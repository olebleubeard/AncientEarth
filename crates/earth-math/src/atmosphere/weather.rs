//! NOAA GFS request planning and strict scientific GRIB ingestion.
use super::*;
use crate::astro::AstroInstant;
use grib::LatLons;
use std::{collections::BTreeMap, io::Cursor};
pub mod custom;
pub mod hrrr;
pub use custom::{CustomWeatherDraft, CustomWeatherRow};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherSample {
    pub height_m: f64,
    pub temperature_k: f64,
    pub pressure_pa: f64,
    pub specific_humidity: f64,
}
#[derive(Clone, Copy, Debug)]
pub enum WeatherGrid {
    Gfs,
    Hrrr { grid_x: u32, grid_y: u32 },
}
#[derive(Clone, Debug)]
pub struct WeatherRequest {
    pub key: String,
    pub url: String,
    pub gfs: GfsRequest,
    pub hrrr: Option<hrrr::HrrrRequest>,
}
impl WeatherRequest {
    pub fn matches_location(&self, location: Geodetic) -> bool {
        self.hrrr.as_ref().map_or_else(
            || self.gfs.matches_location(location),
            |r| r.matches_location(location),
        )
    }
}
#[derive(Clone, Debug)]
pub struct WeatherProfile {
    pub profile: AtmosphereProfile,
    pub metadata: WeatherMetadata,
    pub request: WeatherRequest,
    pub regional_fallback: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GfsRequest {
    pub url: String,
    pub key: String,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub cycle_unix_seconds: i64,
    pub valid_unix_seconds: i64,
    pub forecast_hour: u32,
}
#[derive(Clone, Debug)]
pub struct WeatherMetadata {
    /// Original provider metadata for a user-edited column. The outer source
    /// is custom; this boxed original is provenance, not current observations.
    pub custom_origin: Option<Box<WeatherMetadata>>,
    pub source_grid: WeatherGrid,
    pub source: String,
    pub model: String,
    pub source_url: String,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub cycle_unix_seconds: i64,
    pub valid_unix_seconds: i64,
    pub provider_surface_geopotential_m: f64,
    pub temperature_2m_k: f64,
    pub pressure_surface_pa: f64,
    pub specific_humidity_2m: f64,
    pub source_levels: usize,
    pub extrapolated_refractivity: bool,
    pub temperature_80m_k: Option<f64>,
    pub pressure_80m_pa: Option<f64>,
    pub specific_humidity_80m: Option<f64>,
    pub secondary_source_url: Option<String>,
    pub vertical_input_description: String,
    pub cross_model_join: Option<WeatherJoin>,
}
/// Auditable cross-model registration. These limits are application policy,
/// not a claim that accepted columns reproduce a measured inversion.
pub const MAX_PROVIDER_GROUND_OFFSET_M: f64 = 100.0;
pub const MAX_FIRST_GLOBAL_JOIN_HEIGHT_M: f64 = 200.0;
pub const MIN_CROSS_MODEL_JOIN_SPAN_M: f64 = 20.0;
#[derive(Clone, Copy, Debug)]
pub struct WeatherJoin {
    pub gfs_surface_geopotential_m: f64,
    pub gfs_cycle_unix_seconds: i64,
    pub provider_ground_offset_m: f64,
    pub first_global_height_m: f64,
    pub join_span_m: f64,
}
impl WeatherMetadata {
    /// Friendly source-condition display; unit conversion and rounding do not
    /// change the profile or imply weather measurement precision. Specific
    /// humidity remains mass fraction, never mislabeled as relative humidity.
    pub fn surface_conditions_label(&self, units: UnitSystem) -> String {
        if self.custom_origin.is_some() {
            return format!(
                "Custom column: {} editable levels; temperature, pressure and specific humidity are user-defined.",
                self.source_levels
            );
        }
        let celsius = self.temperature_2m_k - 273.15;
        let (temperature, symbol) = match units {
            UnitSystem::Metric => (celsius, "°C"),
            UnitSystem::Imperial => (celsius * 1.8 + 32.0, "°F"),
        };
        format!(
            "2m air: {temperature:.1}{symbol}; surface: {:.1} hPa; specific humidity: {:.2} g/kg",
            self.pressure_surface_pa / 100.0,
            self.specific_humidity_2m * 1000.0
        )
    }
    pub fn usable_at(&self, location: Geodetic, now_unix_seconds: f64) -> bool {
        let same_grid = match self.source_grid {
            WeatherGrid::Gfs => grid_point(location.lat_deg, location.lon_deg)
                .is_ok_and(|(lat, lon)| lat == self.latitude_deg && lon == self.longitude_deg),
            WeatherGrid::Hrrr { grid_x, grid_y } => {
                hrrr::grid_point(location.lat_deg, location.lon_deg)
                    .is_ok_and(|p| (p.0, p.1) == (grid_x, grid_y))
            }
        };
        same_grid
            && now_unix_seconds.is_finite()
            && (now_unix_seconds - self.valid_unix_seconds as f64).abs() <= 10800.0
    }
    pub fn valid_time_label(&self) -> String {
        AstroInstant::from_unix_utc(self.valid_unix_seconds as f64)
            .map(|i| i.calendar_string())
            .unwrap_or_else(|_| "invalid".into())
    }
    pub fn cycle_time_label(&self) -> String {
        AstroInstant::from_unix_utc(self.cycle_unix_seconds as f64)
            .map(|i| i.calendar_string())
            .unwrap_or_else(|_| "invalid".into())
    }
}
#[derive(Clone, Debug)]
pub struct GfsProfile {
    pub profile: AtmosphereProfile,
    pub metadata: WeatherMetadata,
    pub request: GfsRequest,
    pub samples: Vec<WeatherSample>,
}

fn grid_point(latitude: f64, longitude: f64) -> Result<(f64, f64), AtmosphereError> {
    Geodetic::new(latitude, longitude, 0.0).map_err(|_| AtmosphereError::InvalidSample)?;
    let lat = (latitude * 4.0).round() / 4.0;
    let lon = ((longitude.rem_euclid(360.0) * 4.0).round() / 4.0).rem_euclid(360.0);
    Ok((lat, if lon > 180.0 { lon - 360.0 } else { lon }))
}
pub fn request_candidates(
    latitude: f64,
    longitude: f64,
    now_unix_seconds: f64,
) -> Result<Vec<GfsRequest>, AtmosphereError> {
    let (lat, lon) = grid_point(latitude, longitude)?;
    let date = AstroInstant::from_unix_utc(now_unix_seconds)
        .map_err(|_| AtmosphereError::InvalidSample)?
        .calendar();
    if date.year < 2020 {
        return Err(AtmosphereError::InvalidSample);
    }
    let valid = (now_unix_seconds / 3600.0).round() as i64 * 3600;
    let latest = ((now_unix_seconds - 21600.0) / 21600.0).floor() as i64 * 21600;
    let mut result = Vec::new();
    for offset in 0..3 {
        let cycle = latest - offset * 21600;
        let date = AstroInstant::from_unix_utc(cycle as f64)
            .map_err(|_| AtmosphereError::InvalidSample)?
            .calendar();
        let forecast = ((valid - cycle) / 3600) as u32;
        let lon360 = lon.rem_euclid(360.0);
        let west = (lon360 - 0.25).max(0.0);
        let east = (lon360 + 0.25).min(359.75);
        let south = (lat - 0.25).max(-90.0);
        let north = (lat + 0.25).min(90.0);
        let day = format!("{:04}{:02}{:02}", date.year, date.month, date.day);
        let url = format!(
            "https://nomads.ncep.noaa.gov/cgi-bin/filter_gfs_0p25.pl?file=gfs.t{:02}z.pgrb2.0p25.f{forecast:03}&all_lev=on&var_TMP=on&var_SPFH=on&var_HGT=on&var_PRES=on&subregion=&leftlon={west:.2}&rightlon={east:.2}&toplat={north:.2}&bottomlat={south:.2}&dir=%2Fgfs.{day}%2F{:02}%2Fatmos",
            date.hour, date.hour
        );
        result.push(GfsRequest {
            url,
            key: format!(
                "gfs-{day}-{:02}-f{forecast:03}-{lat:.2}-{lon:.2}",
                date.hour
            ),
            latitude_deg: lat,
            longitude_deg: lon,
            cycle_unix_seconds: cycle,
            valid_unix_seconds: valid,
            forecast_hour: forecast,
        });
    }
    Ok(result)
}
impl GfsRequest {
    pub fn matches_location(&self, location: Geodetic) -> bool {
        grid_point(location.lat_deg, location.lon_deg)
            .is_ok_and(|p| p == (self.latitude_deg, self.longitude_deg))
    }
}

/// Frozen independently decoded NOAA forecast used only by explicitly selected
/// verification mode. It is never a live-weather fallback.
pub fn reference_request() -> GfsRequest {
    request_candidates(29.25, -94.75, 1_792_609_200.0)
        .expect("fixed valid reference request")
        .remove(0)
}
/// Explicit paired NOAA source snapshot; never a live-data fallback.
pub fn regional_reference_request() -> WeatherRequest {
    let gfs = request_candidates(29.305549, -94.767015, 1_792_918_800.0)
        .expect("fixed valid date")
        .remove(0);
    let mut hrrr = hrrr::request_candidates(29.305549, -94.767015, 1_792_918_800.0)
        .expect("fixed HRRR location")
        .remove(1);
    hrrr.url="https://nomads.ncep.noaa.gov/cgi-bin/filter_hrrr_2d.pl?file=hrrr.t06z.wrfsfcf03.grib2&lev_surface=on&lev_2_m_above_ground=on&var_TMP=on&var_SPFH=on&var_HGT=on&var_PRES=on&subregion=&leftlon=-94.87&rightlon=-94.67&toplat=29.41&bottomlat=29.21&dir=%2Fhrrr.20261025%2Fconus".into();
    WeatherRequest {
        key: format!("{}+{}", hrrr.key, gfs.key),
        url: gfs.url.clone(),
        gfs,
        hrrr: Some(hrrr),
    }
}

/// Explicit optical verification pose at the frozen source column. The pitch
/// lies between independently checked straight and refracted skyline angles;
/// this is a diagnostic, not a catalog of real-world locations.
pub fn reference_observer() -> Observer {
    let mut observer = Observer::new(Geodetic::new(29.25, -94.75, 6.0).expect("fixed valid pose"));
    observer.heading_deg = 90.0;
    observer.pitch_deg = -0.085655878886;
    observer
}

pub fn reference_lens() -> Lens {
    Lens::new(Projection::Rectilinear, 0.2)
}

#[derive(Default)]
struct Sample {
    temperature: Option<f64>,
    humidity: Option<f64>,
    height: Option<f64>,
    pressure: Option<f64>,
}
fn rejected(message: impl Into<String>) -> AtmosphereError {
    AtmosphereError::InvalidWeather(message.into())
}
pub fn decode_gfs(bytes: &[u8], request: &GfsRequest) -> Result<GfsProfile, AtmosphereError> {
    if request
        .cycle_unix_seconds
        .checked_add(i64::from(request.forecast_hour) * 3600)
        != Some(request.valid_unix_seconds)
    {
        return Err(rejected("request cycle and valid time disagree"));
    }
    if bytes.len() > 8_000_000 || bytes.len() < 16 || &bytes[..4] != b"GRIB" {
        return Err(rejected("not a bounded GRIB2 response"));
    }
    let parsed = grib::from_reader(Cursor::new(bytes)).map_err(|e| rejected(e.to_string()))?;
    let expected = AstroInstant::from_unix_utc(request.cycle_unix_seconds as f64)
        .map_err(|_| AtmosphereError::InvalidSample)?
        .calendar();
    let mut levels = BTreeMap::<u32, Sample>::new();
    let mut surface = Sample::default();
    let mut air = Sample::default();
    let mut air80 = Sample::default();
    let mut pressure = None;
    for (_, message) in parsed.iter() {
        if message.indicator().discipline != 0 {
            continue;
        }
        let product = message.prod_def();
        let Some((category, parameter)) =
            product.parameter_category().zip(product.parameter_number())
        else {
            continue;
        };
        if ![(0, 0), (1, 0), (3, 0), (3, 5)].contains(&(category, parameter)) {
            continue;
        }
        let Some((fixed, _)) = product.fixed_surfaces() else {
            continue;
        };
        if fixed.surface_type != 100
            && fixed.surface_type != 1
            && !(fixed.surface_type == 103 && [2.0, 80.0].contains(&fixed.value()))
        {
            continue;
        }
        if message.identification().centre_id() != 7 || product.prod_tmpl_num() != 0 {
            return Err(rejected("expected instantaneous NOAA/NCEP forecast fields"));
        }
        let reference = message.identification().ref_time_unchecked();
        if i32::from(reference.year) != expected.year
            || u32::from(reference.month) != expected.month
            || u32::from(reference.day) != expected.day
            || u32::from(reference.hour) != expected.hour
            || reference.minute != 0
            || reference.second != 0
            || product.forecast_time()
                != Some(grib::ForecastTime::from_numbers(1, request.forecast_hour))
        {
            return Err(rejected("mixed or unexpected model cycle/forecast time"));
        }
        if message.grid_def().grid_tmpl_num() != 0 || message.grid_def().num_points() > 10000 {
            return Err(rejected("unsupported or oversized source grid"));
        }
        let points: Vec<_> = message
            .latlons()
            .map_err(|e| rejected(e.to_string()))?
            .take(10001)
            .collect();
        if points.len() > 10000 {
            return Err(rejected("unexpected global grid response"));
        }
        let selected = points
            .iter()
            .position(|&(lat, lon)| {
                (f64::from(lat) - request.latitude_deg).abs() < 1e-6
                    && ((f64::from(lon) - request.longitude_deg + 180.0).rem_euclid(360.0) - 180.0)
                        .abs()
                        < 1e-6
            })
            .ok_or_else(|| rejected("requested source grid column is absent"))?;
        let decoder =
            grib::Grib2SubmessageDecoder::from(message).map_err(|e| rejected(e.to_string()))?;
        let value = f64::from(
            decoder
                .dispatch()
                .map_err(|e| rejected(e.to_string()))?
                .nth(selected)
                .ok_or_else(|| rejected("truncated grid values"))?,
        );
        if !value.is_finite() {
            return Err(rejected("missing required source value"));
        }
        if fixed.surface_type == 1 && (category, parameter) == (3, 0) {
            if pressure.replace(value).is_some() {
                return Err(rejected("duplicate surface pressure"));
            }
            continue;
        }
        let sample = if fixed.surface_type == 100 {
            let p = fixed.value();
            if !p.is_finite() || !(1.0..=110000.0).contains(&p) || p.fract() != 0.0 {
                continue;
            }
            levels.entry(p as u32).or_default()
        } else if fixed.surface_type == 103 {
            if fixed.value() == 80.0 {
                &mut air80
            } else {
                &mut air
            }
        } else {
            &mut surface
        };
        let slot = match (category, parameter) {
            (0, 0) => &mut sample.temperature,
            (1, 0) => &mut sample.humidity,
            (3, 5) => &mut sample.height,
            (3, 0) => &mut sample.pressure,
            _ => continue,
        };
        if slot.replace(value).is_some() {
            return Err(rejected("duplicate required field"));
        }
    }
    let ground = surface
        .height
        .ok_or_else(|| rejected("surface geopotential height missing"))?;
    let temperature = air
        .temperature
        .ok_or_else(|| rejected("2m air temperature missing"))?;
    let humidity = air
        .humidity
        .ok_or_else(|| rejected("2m specific humidity missing"))?;
    let pressure = pressure.ok_or_else(|| rejected("actual surface pressure missing"))?;
    if !(-1000.0..=10000.0).contains(&ground) {
        return Err(rejected("surface height outside provider domain"));
    }
    // Explicit coordinate mapping: convert geopotential heights using the
    // conventional geopotential radius, then subtract the provider ground.
    // This is a transplanted profile, not restored terrain or local observation.
    let geometric = |h: f64| 6_356_766.0 * h / (6_356_766.0 - h);
    let ground_geometric = geometric(ground);
    let virtual_temperature = temperature * (1.0 + 0.6078 * humidity);
    let p2 = pressure * (-9.80665 * 2.0 / (287.05 * virtual_temperature)).exp();
    let mut samples = vec![
        (0.0, temperature, pressure, humidity),
        (2.0, temperature, p2, humidity),
    ];
    let (Some(t80), Some(q80), Some(p80)) = (air80.temperature, air80.humidity, air80.pressure)
    else {
        return Err(rejected("complete 80m TMP/SPFH/PRES triplet missing"));
    };
    if p80 >= p2 || p80 <= 0.0 {
        return Err(rejected(
            "80m pressure inconsistent with source surface pressure",
        ));
    }
    samples.push((80.0, t80, p80, q80));
    for (p, sample) in levels.into_iter().rev() {
        let (Some(t), Some(q), Some(h)) = (sample.temperature, sample.humidity, sample.height)
        else {
            return Err(rejected("incomplete pressure-level triplet"));
        };
        if !(-1000.0..=110000.0).contains(&h) {
            return Err(rejected(
                "pressure-level geopotential outside supported domain",
            ));
        }
        let height = geometric(h) - ground_geometric;
        if !height.is_finite() {
            return Err(rejected("invalid mapped source altitude"));
        }
        if p as f64 >= p2 || height <= 2.01 {
            continue;
        }
        if height > 110000.0 || !height.is_finite() {
            return Err(rejected("pressure level outside altitude mapping"));
        }
        // Preserve the explicitly supplied AGL knot when a pressure-level
        // interpolation happens to register within one centimetre of it.
        if (height - 80.0).abs() > 0.01 {
            samples.push((height, t, p as f64, q));
        }
    }
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    if samples.len() < 12 || samples.last().unwrap().0 < 60000.0 {
        return Err(rejected("insufficient vertical coverage"));
    }
    let source_levels = samples.len();
    let extrapolated = samples
        .iter()
        .any(|&(_, t, p, _)| !(233.15..=373.15).contains(&t) || !(80000.0..=120000.0).contains(&p));
    let samples: Vec<_> = samples
        .into_iter()
        .map(
            |(height_m, temperature_k, pressure_pa, specific_humidity)| WeatherSample {
                height_m,
                temperature_k,
                pressure_pa,
                specific_humidity,
            },
        )
        .collect();
    let metadata = WeatherMetadata {
        custom_origin: None,
        source_grid: WeatherGrid::Gfs,
        source: "NOAA NCEP NOMADS".into(),
        model: "GFS 0.25 degree forecast column".into(),
        source_url: request.url.clone(),
        latitude_deg: request.latitude_deg,
        longitude_deg: request.longitude_deg,
        cycle_unix_seconds: request.cycle_unix_seconds,
        valid_unix_seconds: request.valid_unix_seconds,
        provider_surface_geopotential_m: ground,
        temperature_2m_k: temperature,
        pressure_surface_pa: pressure,
        specific_humidity_2m: humidity,
        source_levels,
        extrapolated_refractivity: extrapolated,
        temperature_80m_k: Some(t80),
        pressure_80m_pa: Some(p80),
        specific_humidity_80m: Some(q80),
        secondary_source_url: None,
        vertical_input_description:
            "2m air, actual 80m TMP/SPFH/PRES, pressure levels; fixed column transplant".into(),
        cross_model_join: None,
    };
    let profile = build_profile(&samples, metadata.clone())?;
    Ok(GfsProfile {
        profile,
        metadata,
        request: request.clone(),
        samples,
    })
}

pub fn weather_requests(
    latitude: f64,
    longitude: f64,
    now: f64,
) -> Result<Vec<WeatherRequest>, AtmosphereError> {
    let regional = hrrr::request_candidates(latitude, longitude, now).unwrap_or_default();
    Ok(request_candidates(latitude, longitude, now)?
        .into_iter()
        .map(|gfs| {
            let hrrr = regional
                .iter()
                .find(|h| h.valid_unix_seconds == gfs.valid_unix_seconds)
                .cloned();
            WeatherRequest {
                key: hrrr
                    .as_ref()
                    .map_or_else(|| gfs.key.clone(), |h| format!("{}+{}", h.key, gfs.key)),
                url: gfs.url.clone(),
                gfs,
                hrrr,
            }
        })
        .collect())
}
pub fn decode_weather(
    gfs_bytes: &[u8],
    hrrr_bytes: Option<&[u8]>,
    request: &WeatherRequest,
) -> Result<WeatherProfile, AtmosphereError> {
    let global = decode_gfs(gfs_bytes, &request.gfs)?;
    let composite = || -> Result<AtmosphereProfile, AtmosphereError> {
        let regional = request
            .hrrr
            .as_ref()
            .ok_or_else(|| rejected("regional column not requested"))?;
        let bytes = hrrr_bytes.ok_or_else(|| {
            rejected("regional data unavailable; validated global column retained")
        })?;
        let lower = hrrr::decode_hrrr(bytes, regional)?;
        composite_profile(&global, lower)
    };
    if request.hrrr.is_some() {
        match composite() {
            Ok(profile) => {
                return Ok(WeatherProfile {
                    metadata: profile.metadata.clone().unwrap(),
                    profile,
                    request: request.clone(),
                    regional_fallback: None,
                });
            }
            Err(error) => {
                return Ok(WeatherProfile {
                    profile: global.profile,
                    metadata: global.metadata,
                    request: request.clone(),
                    regional_fallback: Some(error.to_string()),
                });
            }
        }
    }
    Ok(WeatherProfile {
        profile: global.profile,
        metadata: global.metadata,
        request: request.clone(),
        regional_fallback: None,
    })
}

fn composite_profile(
    global: &GfsProfile,
    lower: hrrr::HrrrColumn,
) -> Result<AtmosphereProfile, AtmosphereError> {
    if lower.metadata.valid_unix_seconds != global.metadata.valid_unix_seconds {
        return Err(rejected("regional/global forecast valid times differ"));
    }
    let geometric = |h: f64| 6_356_766.0 * h / (6_356_766.0 - h);
    let offset = geometric(global.metadata.provider_surface_geopotential_m)
        - geometric(lower.metadata.provider_surface_geopotential_m);
    if !offset.is_finite() || offset.abs() > MAX_PROVIDER_GROUND_OFFSET_M {
        return Err(rejected(
            "provider ground-height difference exceeds the 100m composite policy; global column retained",
        ));
    }
    let mut samples = lower.samples;
    let top = samples
        .last()
        .ok_or_else(|| rejected("empty regional column"))?
        .height_m;
    let joined: Vec<_> = global
        .samples
        .iter()
        .filter_map(|sample| {
            // HRRR owns the lower surface/2m convention. Retaining the competing
            // synthetic GFS anchors after a small datum offset would invent an
            // arbitrarily thin cross-model layer. Only actual upper input joins.
            if sample.height_m <= 2.01 {
                return None;
            }
            let mut registered = *sample;
            registered.height_m += offset;
            (registered.height_m > top + 0.01).then_some(registered)
        })
        .collect();
    let first = joined
        .first()
        .ok_or_else(|| rejected("no global column above regional source"))?
        .height_m;
    if first - top < MIN_CROSS_MODEL_JOIN_SPAN_M {
        return Err(rejected(
            "cross-model join is shorter than the 20m composite policy; global column retained",
        ));
    }
    if first > MAX_FIRST_GLOBAL_JOIN_HEIGHT_M {
        return Err(rejected(
            "first global join exceeds the 200m composite policy; global column retained",
        ));
    }
    samples.extend(joined);
    let mut metadata = lower.metadata;
    metadata.model = "HRRR 3 km near-surface + GFS 0.25 degree vertical column".into();
    metadata.secondary_source_url = Some(global.request.url.clone());
    metadata.source_levels = samples.len();
    metadata.extrapolated_refractivity |= global.metadata.extrapolated_refractivity;
    metadata.cross_model_join = Some(WeatherJoin {
        gfs_surface_geopotential_m: global.metadata.provider_surface_geopotential_m,
        gfs_cycle_unix_seconds: global.metadata.cycle_unix_seconds,
        provider_ground_offset_m: offset,
        first_global_height_m: first,
        join_span_m: first - top,
    });
    metadata.vertical_input_description = format!(
        "HRRR surface/2m; modeled cross-model linear join to GFS at {first:.2}m AGL (span {:.2}m, ground offset {offset:+.2}m); fixed column transplant",
        first - top
    );
    build_profile(&samples, metadata)
}

pub(crate) fn build_profile(
    samples: &[WeatherSample],
    metadata: WeatherMetadata,
) -> Result<AtmosphereProfile, AtmosphereError> {
    if samples.len() < 12
        || samples[0].height_m != 0.0
        || samples
            .last()
            .is_none_or(|s| s.height_m < 60000.0 || s.height_m >= 120000.0)
        || samples
            .windows(2)
            .any(|p| p[1].height_m - p[0].height_m < 0.01 || p[1].pressure_pa >= p[0].pressure_pa)
    {
        return Err(rejected(
            "incomplete/nonmonotonic atmospheric height and pressure registration",
        ));
    }
    let mut layers = Vec::with_capacity(samples.len() + 1);
    for sample in samples {
        if !sample.height_m.is_finite() {
            return Err(rejected("nonfinite source height"));
        }
        layers.push(AtmosphereLayer {
            height_m: sample.height_m,
            refractivity: phase_index(
                sample.temperature_k,
                sample.pressure_pa,
                sample.specific_humidity,
                550.0,
            )? - 1.0,
        });
    }
    layers.push(AtmosphereLayer {
        height_m: 120000.0,
        refractivity: 0.0,
    });
    let mut profile = AtmosphereProfile::from_layers(layers)?;
    profile.weather_samples = Some(samples.to_vec());
    profile.metadata = Some(metadata);
    Ok(profile)
}

#[cfg(test)]
mod join_policy_tests {
    use super::*;
    fn columns() -> (GfsProfile, hrrr::HrrrColumn) {
        let request = regional_reference_request();
        (
            decode_gfs(
                include_bytes!("../../../../assets/weather/gfs-regional-reference.grib2"),
                &request.gfs,
            )
            .unwrap(),
            hrrr::decode_hrrr(
                include_bytes!("../../../../assets/weather/hrrr-reference.grib2"),
                request.hrrr.as_ref().unwrap(),
            )
            .unwrap(),
        )
    }
    #[test]
    fn registered_pair_exposes_actual_join_and_preserves_both_provider_heights() {
        let (global, regional) = columns();
        let regional_ground = regional.metadata.provider_surface_geopotential_m;
        let profile = composite_profile(&global, regional).unwrap();
        let metadata = profile.metadata.unwrap();
        let join = metadata.cross_model_join.unwrap();
        assert_eq!(metadata.provider_surface_geopotential_m, regional_ground);
        assert_eq!(
            join.gfs_surface_geopotential_m,
            global.metadata.provider_surface_geopotential_m
        );
        assert_eq!(
            join.gfs_cycle_unix_seconds,
            global.metadata.cycle_unix_seconds
        );
        assert!(join.provider_ground_offset_m.abs() <= MAX_PROVIDER_GROUND_OFFSET_M);
        assert!(join.first_global_height_m <= MAX_FIRST_GLOBAL_JOIN_HEIGHT_M);
        assert!(join.join_span_m >= MIN_CROSS_MODEL_JOIN_SPAN_M);
        assert!((join.join_span_m - (join.first_global_height_m - 2.0)).abs() < 1e-12);
        assert!(
            (join.first_global_height_m - (80.0 + join.provider_ground_offset_m)).abs() < 1e-12,
            "this source pair's first real upper input is the actual80m triplet"
        );
        println!(
            "Verified source pair: HRRR H={regional_ground:.9}m, GFS H={:.9}m, offset={:.9}m, first GFS={:.9}m, join span={:.9}m",
            join.gfs_surface_geopotential_m,
            join.provider_ground_offset_m,
            join.first_global_height_m,
            join.join_span_m
        );
    }
    #[test]
    fn composite_rejects_large_provider_mismatch_and_long_unobserved_join() {
        let (mut global, regional) = columns();
        global.metadata.provider_surface_geopotential_m =
            regional.metadata.provider_surface_geopotential_m + 101.0;
        assert!(
            composite_profile(&global, regional)
                .unwrap_err()
                .to_string()
                .contains("100m")
        );
        let (mut global, regional) = columns();
        global.samples.retain(|sample| sample.height_m > 400.0);
        assert!(
            composite_profile(&global, regional)
                .unwrap_err()
                .to_string()
                .contains("200m")
        );
        let (mut global, regional) = columns();
        global
            .samples
            .iter_mut()
            .find(|sample| sample.height_m == 80.0)
            .unwrap()
            .height_m = 10.0;
        assert!(
            composite_profile(&global, regional)
                .unwrap_err()
                .to_string()
                .contains("20m")
        );
        let (mut global, regional) = columns();
        global.metadata.provider_surface_geopotential_m =
            regional.metadata.provider_surface_geopotential_m;
        global
            .samples
            .iter_mut()
            .find(|sample| sample.height_m == 80.0)
            .unwrap()
            .height_m = 22.0;
        assert!(
            composite_profile(&global, regional).is_ok(),
            "exact20m span is eligible"
        );
        let (mut global, regional) = columns();
        global.metadata.provider_surface_geopotential_m =
            regional.metadata.provider_surface_geopotential_m;
        global
            .samples
            .iter_mut()
            .find(|sample| sample.height_m == 80.0)
            .unwrap()
            .height_m = f64::from_bits(22.0f64.to_bits() - 1);
        assert!(
            composite_profile(&global, regional)
                .unwrap_err()
                .to_string()
                .contains("20m")
        );
    }
}
