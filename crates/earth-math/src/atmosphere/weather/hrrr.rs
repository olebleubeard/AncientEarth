//! NOAA HRRR CONUS native-grid request planning and bounded GRIB2 ingestion.
//!
//! Only the public wrfsfc subset's actual surface PRES/HGT and 2 m TMP/SPFH
//! are used. The returned two knots require an explicitly compatible GFS
//! vertical column. This is not a full HRRR pressure-level profile; the NOAA
//! subset endpoint rejects wrfprs files. HRRR has no 80 m T/P/q triplet.
use super::{WeatherGrid, WeatherMetadata, WeatherSample, rejected};
use crate::{Geodetic, astro::AstroInstant, atmosphere::AtmosphereError};
use grib::{GridDefinitionTemplateValues, def::grib2::template::Template3_30};
use std::{f64::consts::FRAC_PI_2, io::Cursor};

const RADIUS_M: f64 = 6_371_229.0;
const SPACING_M: f64 = 3_000.0;
const NX: u32 = 1799;
const NY: u32 = 1059;
const FIRST_LATITUDE: f64 = 21.138123;
const FIRST_LONGITUDE: f64 = -122.719528;
const MAX_POINTS: u32 = 10_000;

#[derive(Clone, Debug, PartialEq)]
pub struct HrrrRequest {
    pub url: String,
    pub key: String,
    /// Selected native grid post, not the requested observer location.
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub grid_x: u32,
    pub grid_y: u32,
    pub cycle_unix_seconds: i64,
    pub valid_unix_seconds: i64,
    pub forecast_hour: u32,
}

#[derive(Clone, Debug)]
pub struct HrrrColumn {
    pub samples: Vec<WeatherSample>,
    pub metadata: WeatherMetadata,
    pub request: HrrrRequest,
}

// Spherical tangent-cone Lambert conformal projection. The arbitrary y origin
// is the cone apex; subtracting the first post removes that false origin.
// GRIB template 3.30 declares the sphere, standard parallel and spacing. These
// formulas are also independently checked against PROJ, not WGS84 distances.
fn cone() -> (f64, f64) {
    let phi = 38.5_f64.to_radians();
    let n = phi.sin();
    let factor = RADIUS_M * phi.cos() / n * (std::f64::consts::FRAC_PI_4 + phi * 0.5).tan().powf(n);
    (n, factor)
}

fn project(latitude: f64, longitude: f64) -> Result<[f64; 2], AtmosphereError> {
    Geodetic::new(latitude, longitude, 0.0).map_err(|_| AtmosphereError::InvalidSample)?;
    if latitude.abs() >= 89.0 {
        return Err(rejected("outside HRRR CONUS projection domain"));
    }
    let (n, factor) = cone();
    let phi = latitude.to_radians();
    let theta = n * ((longitude + 97.5 + 180.0).rem_euclid(360.0) - 180.0).to_radians();
    let rho = factor / (std::f64::consts::FRAC_PI_4 + phi * 0.5).tan().powf(n);
    Ok([rho * theta.sin(), -rho * theta.cos()])
}

fn unproject(x: f64, y: f64) -> (f64, f64) {
    let (n, factor) = cone();
    let latitude = (2.0 * (factor / x.hypot(y)).powf(1.0 / n).atan() - FRAC_PI_2).to_degrees();
    let longitude = (x.atan2(-y) / n).to_degrees() - 97.5;
    (latitude, (longitude + 180.0).rem_euclid(360.0) - 180.0)
}

fn grid_coordinates(latitude: f64, longitude: f64) -> Result<[f64; 2], AtmosphereError> {
    let point = project(latitude, longitude)?;
    let first = project(FIRST_LATITUDE, FIRST_LONGITUDE)?;
    Ok([
        (point[0] - first[0]) / SPACING_M,
        (point[1] - first[1]) / SPACING_M,
    ])
}

/// Nearest post in the native Lambert projected plane (half-cell ties toward
/// increasing indices). No interpolation or extrapolation beyond the post box.
pub fn grid_point(latitude: f64, longitude: f64) -> Result<(u32, u32, f64, f64), AtmosphereError> {
    let [x, y] = grid_coordinates(latitude, longitude)?;
    // 1e-8 cell (30 micrometres) covers f64 projection roundoff at the
    // published edge posts; this is not geographic outside-grid extrapolation.
    if !(-1e-8..=f64::from(NX - 1) + 1e-8).contains(&x)
        || !(-1e-8..=f64::from(NY - 1) + 1e-8).contains(&y)
    {
        return Err(rejected("location outside HRRR CONUS native grid"));
    }
    let i = x.clamp(0.0, f64::from(NX - 1)).round() as u32;
    let j = y.clamp(0.0, f64::from(NY - 1)).round() as u32;
    let first = project(FIRST_LATITUDE, FIRST_LONGITUDE)?;
    let (lat, lon) = unproject(
        first[0] + f64::from(i) * SPACING_M,
        first[1] + f64::from(j) * SPACING_M,
    );
    Ok((i, j, lat, lon))
}

impl HrrrRequest {
    pub fn matches_location(&self, location: Geodetic) -> bool {
        grid_point(location.lat_deg, location.lon_deg)
            .is_ok_and(|(i, j, _, _)| i == self.grid_x && j == self.grid_y)
    }
}

/// Explicit frozen source verification request, never a live-weather fallback.
pub fn reference_request() -> HrrrRequest {
    let mut request = request_candidates(29.305549, -94.767015, 1_792_918_800.0)
        .expect("valid frozen HRRR reference")
        .remove(1);
    request.url = "https://nomads.ncep.noaa.gov/cgi-bin/filter_hrrr_2d.pl?file=hrrr.t06z.wrfsfcf03.grib2&lev_surface=on&lev_2_m_above_ground=on&var_TMP=on&var_SPFH=on&var_HGT=on&var_PRES=on&subregion=&leftlon=-94.87&rightlon=-94.67&toplat=29.41&bottomlat=29.21&dir=%2Fhrrr.20261025%2Fconus".into();
    request
}

/// Three hourly cycles, starting two hours behind wall time. This is an
/// availability policy, not a guarantee that NOAA has published each cycle.
pub fn request_candidates(
    latitude: f64,
    longitude: f64,
    now_unix_seconds: f64,
) -> Result<Vec<HrrrRequest>, AtmosphereError> {
    let (i, j, lat, lon) = grid_point(latitude, longitude)?;
    let instant = AstroInstant::from_unix_utc(now_unix_seconds)
        .map_err(|_| AtmosphereError::InvalidSample)?;
    if instant.calendar().year < 2021 {
        return Err(AtmosphereError::InvalidSample);
    }
    let valid = (now_unix_seconds / 3600.0).round() as i64 * 3600;
    let latest = ((now_unix_seconds - 7200.0) / 3600.0).floor() as i64 * 3600;
    let mut requests = Vec::new();
    for offset in 0..3 {
        let cycle = latest - offset * 3600;
        let date = AstroInstant::from_unix_utc(cycle as f64)
            .map_err(|_| AtmosphereError::InvalidSample)?
            .calendar();
        let forecast = ((valid - cycle) / 3600) as u32;
        let day = format!("{:04}{:02}{:02}", date.year, date.month, date.day);
        // Several source posts of padding make the selected post an interior
        // member of the subset despite microdegree source-header rounding.
        let west = lon - 0.12;
        let east = lon + 0.12;
        let south = lat - 0.12;
        let north = lat + 0.12;
        requests.push(HrrrRequest {
            url: format!("https://nomads.ncep.noaa.gov/cgi-bin/filter_hrrr_2d.pl?file=hrrr.t{:02}z.wrfsfcf{forecast:02}.grib2&lev_surface=on&lev_2_m_above_ground=on&var_TMP=on&var_SPFH=on&var_HGT=on&var_PRES=on&subregion=&leftlon={west:.6}&rightlon={east:.6}&toplat={north:.6}&bottomlat={south:.6}&dir=%2Fhrrr.{day}%2Fconus", date.hour),
            key: format!("hrrr-surface-v1-{day}-{:02}-f{forecast:02}-{i}-{j}", date.hour),
            latitude_deg: lat, longitude_deg: lon, grid_x: i, grid_y: j,
            cycle_unix_seconds: cycle, valid_unix_seconds: valid, forecast_hour: forecast,
        });
    }
    Ok(requests)
}

fn selected_index(
    grid: &Template3_30,
    points: u32,
    request: &HrrrRequest,
) -> Result<usize, AtmosphereError> {
    if grid.earth_shape.shape != 6
        || grid.lad != 38_500_000
        || grid.latin1 != 38_500_000
        || grid.latin2 != 38_500_000
        || grid.lov != 262_500_000
        || grid.dx != 3_000_000
        || grid.dy != 3_000_000
        || grid.projection_centre.0 != 0
        || grid.scanning_mode.0 != 64
        || grid.ni == 0
        || grid.nj == 0
        || grid.first_point_lon > 360_000_000
        || points > MAX_POINTS
        || grid.ni.checked_mul(grid.nj) != Some(points)
    {
        return Err(rejected(
            "unexpected HRRR Lambert grid registration or size",
        ));
    }
    let origin = grid_coordinates(
        f64::from(grid.first_point_lat) * 1e-6,
        (f64::from(grid.first_point_lon) * 1e-6 + 180.0).rem_euclid(360.0) - 180.0,
    )?;
    let ix = origin[0].round();
    let iy = origin[1].round();
    // Each first-post latitude/longitude is rounded to a microdegree. 0.25 m
    // covers that registration quantization, not arbitrary grid displacement.
    if (origin[0] - ix).abs() * SPACING_M > 0.25
        || (origin[1] - iy).abs() * SPACING_M > 0.25
        || ix < 0.0
        || iy < 0.0
        || ix + f64::from(grid.ni) > f64::from(NX)
        || iy + f64::from(grid.nj) > f64::from(NY)
    {
        return Err(rejected("HRRR subset is not aligned to the native lattice"));
    }
    let x = i64::from(request.grid_x) - ix as i64;
    let y = i64::from(request.grid_y) - iy as i64;
    if x < 0 || y < 0 || x >= i64::from(grid.ni) || y >= i64::from(grid.nj) {
        return Err(rejected("selected HRRR column is absent from subset"));
    }
    Ok((y * i64::from(grid.ni) + x) as usize)
}

#[derive(Default)]
struct Sample {
    temperature: Option<f64>,
    humidity: Option<f64>,
}

pub fn decode_hrrr(bytes: &[u8], request: &HrrrRequest) -> Result<HrrrColumn, AtmosphereError> {
    let (i, j, lat, lon) = grid_point(request.latitude_deg, request.longitude_deg)?;
    if i != request.grid_x
        || j != request.grid_y
        || (lat - request.latitude_deg).abs() > 1e-10
        || (lon - request.longitude_deg).abs() > 1e-10
        || request.forecast_hour > 18
        || request.cycle_unix_seconds.rem_euclid(3600) != 0
        || request
            .cycle_unix_seconds
            .checked_add(i64::from(request.forecast_hour) * 3600)
            != Some(request.valid_unix_seconds)
    {
        return Err(rejected("inconsistent HRRR request identity"));
    }
    if !(16..=8_000_000).contains(&bytes.len()) || &bytes[..4] != b"GRIB" {
        return Err(rejected("not a bounded GRIB2 response"));
    }
    let parsed = grib::from_reader(Cursor::new(bytes)).map_err(|e| rejected(e.to_string()))?;
    let expected = AstroInstant::from_unix_utc(request.cycle_unix_seconds as f64)
        .map_err(|_| AtmosphereError::InvalidSample)?
        .calendar();
    let mut surface_height = None;
    let mut surface_pressure = None;
    let mut air = Sample::default();
    let mut grid_identity = None;
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
        let Some((fixed, _)) = product.fixed_surfaces() else {
            continue;
        };
        let field = (category, parameter);
        let required = match fixed.surface_type {
            1 => [(3, 0), (3, 5)].contains(&field),
            103 if fixed.value() == 2.0 => [(0, 0), (1, 0)].contains(&field),
            _ => false,
        };
        if !required {
            continue;
        }
        if message.identification().centre_id() != 7 || product.prod_tmpl_num() != 0 {
            return Err(rejected("expected instantaneous NOAA HRRR fields"));
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
            return Err(rejected("mixed or unexpected HRRR cycle/forecast"));
        }
        let grid_definition = message.grid_def();
        if grid_definition.grid_tmpl_num() != 30 || grid_definition.num_points() > MAX_POINTS {
            return Err(rejected("expected bounded native HRRR Lambert subset"));
        }
        let grid = GridDefinitionTemplateValues::try_from(grid_definition)
            .map_err(|e| rejected(e.to_string()))?;
        let GridDefinitionTemplateValues::Template30(grid) = grid else {
            return Err(rejected("not Lambert"));
        };
        let index = selected_index(&grid, grid_definition.num_points(), request)?;
        if let Some(previous) = &grid_identity {
            if previous != &grid {
                return Err(rejected("mixed HRRR grids"));
            }
        } else {
            grid_identity = Some(grid);
        }
        let decoder =
            grib::Grib2SubmessageDecoder::from(message).map_err(|e| rejected(e.to_string()))?;
        let value = f64::from(
            decoder
                .dispatch()
                .map_err(|e| rejected(e.to_string()))?
                .nth(index)
                .ok_or_else(|| rejected("truncated HRRR field"))?,
        );
        if !value.is_finite() {
            return Err(rejected("missing HRRR source value"));
        }
        let slot = if fixed.surface_type == 1 {
            if field == (3, 0) {
                &mut surface_pressure
            } else {
                &mut surface_height
            }
        } else {
            if field == (0, 0) {
                &mut air.temperature
            } else {
                &mut air.humidity
            }
        };
        if slot.replace(value).is_some() {
            return Err(rejected("duplicate HRRR field"));
        }
    }
    let ground = surface_height.ok_or_else(|| rejected("missing HRRR surface HGT"))?;
    let pressure = surface_pressure.ok_or_else(|| rejected("missing HRRR surface pressure"))?;
    let temperature = air
        .temperature
        .ok_or_else(|| rejected("missing HRRR 2m temperature"))?;
    let humidity = air
        .humidity
        .ok_or_else(|| rejected("missing HRRR 2m specific humidity"))?;
    if !(-1000.0..=10000.0).contains(&ground) || pressure <= 0.0 {
        return Err(rejected("invalid HRRR surface geopotential"));
    }
    crate::atmosphere::phase_index(temperature, pressure, humidity, 550.0)?;
    let p2 = pressure * (-9.80665 * 2.0 / (287.05 * temperature * (1.0 + 0.6078 * humidity))).exp();
    let samples = vec![
        WeatherSample {
            height_m: 0.0,
            temperature_k: temperature,
            pressure_pa: pressure,
            specific_humidity: humidity,
        },
        WeatherSample {
            height_m: 2.0,
            temperature_k: temperature,
            pressure_pa: p2,
            specific_humidity: humidity,
        },
    ];
    let metadata = WeatherMetadata {
        custom_origin: None,
        source_grid: WeatherGrid::Hrrr {
            grid_x: i,
            grid_y: j,
        },
        source: "NOAA NCEP NOMADS".into(),
        model: "HRRR 3km CONUS surface/2m conditions".into(),
        source_url: request.url.clone(),
        latitude_deg: request.latitude_deg,
        longitude_deg: request.longitude_deg,
        cycle_unix_seconds: request.cycle_unix_seconds,
        valid_unix_seconds: request.valid_unix_seconds,
        provider_surface_geopotential_m: ground,
        temperature_2m_k: temperature,
        pressure_surface_pa: pressure,
        specific_humidity_2m: humidity,
        source_levels: samples.len(),
        extrapolated_refractivity: samples.iter().any(|s| {
            !(233.15..=373.15).contains(&s.temperature_k)
                || !(80000.0..=120000.0).contains(&s.pressure_pa)
        }),
        temperature_80m_k: None,
        pressure_80m_pa: None,
        specific_humidity_80m: None,
        secondary_source_url: None,
        cross_model_join: None,
        vertical_input_description: "HRRR 3km surface/2m; GFS vertical column required; 2m pressure hydrostatically estimated".into(),
    };
    Ok(HrrrColumn {
        samples,
        metadata,
        request: request.clone(),
    })
}
