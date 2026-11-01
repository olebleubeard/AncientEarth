//! Editable thermodynamic columns using the existing Ciddor/profile model.
//! Heights and the vacuum continuation retain their source-defined values.
use super::{AtmosphereError, AtmosphereProfile, UnitSystem, WeatherSample, build_profile};

#[derive(Clone, Debug, PartialEq)]
pub struct CustomWeatherRow {
    height_label: String,
    pub temperature: String,
    pub pressure_hpa: String,
    /// Mass of water vapor per mass of moist air, in g/kg; never relative humidity.
    pub specific_humidity_g_kg: String,
}

impl CustomWeatherRow {
    pub fn height_label(&self) -> &str {
        &self.height_label
    }
}

/// Display strings are retained alongside exact SI knots. An untouched field
/// uses its original bits, avoiding a lossy SI -> display -> SI round trip.
#[derive(Clone, Debug)]
pub struct CustomWeatherDraft {
    pub rows: Vec<CustomWeatherRow>,
    units: UnitSystem,
    source: AtmosphereProfile,
    samples: Vec<WeatherSample>,
    displayed: Vec<CustomWeatherRow>,
}

impl CustomWeatherDraft {
    pub fn from_profile(
        profile: &AtmosphereProfile,
        units: UnitSystem,
    ) -> Result<Self, AtmosphereError> {
        let samples = profile.weather_samples().ok_or_else(|| {
            invalid("This profile has no retained temperature, pressure and humidity column.")
        })?;
        if profile.metadata.is_none() {
            return Err(invalid("The source weather provenance is unavailable."));
        }
        let displayed = display_rows(samples, units);
        Ok(Self {
            rows: displayed.clone(),
            units,
            source: profile.clone(),
            samples: samples.to_vec(),
            displayed,
        })
    }

    pub fn units(&self) -> UnitSystem {
        self.units
    }

    pub fn temperature_unit_label(&self) -> &'static str {
        match self.units {
            UnitSystem::Metric => "°C",
            UnitSystem::Imperial => "°F",
        }
    }

    /// Transactional conversion: invalid text leaves every row and unit intact.
    /// Parsed SI is retained so changing units cannot round unedited values.
    pub fn set_units(&mut self, units: UnitSystem) -> Result<(), AtmosphereError> {
        if self.units == units {
            return Ok(());
        }
        let samples = self.parsed_samples()?;
        let displayed = display_rows(&samples, units);
        self.samples = samples;
        self.rows = displayed.clone();
        self.displayed = displayed;
        self.units = units;
        Ok(())
    }

    pub fn build_profile(&self) -> Result<AtmosphereProfile, AtmosphereError> {
        let samples = self.parsed_samples()?;
        let source_metadata = self
            .source
            .metadata
            .as_ref()
            .ok_or_else(|| invalid("The source weather provenance is unavailable."))?;
        // Re-editing a custom column retains the initial provider provenance;
        // it does not recursively wrap previous custom labels as a new source.
        let origin = source_metadata
            .custom_origin
            .as_deref()
            .unwrap_or(source_metadata);
        let mut metadata = origin.clone();
        metadata.custom_origin = Some(Box::new(origin.clone()));
        metadata.source = "Custom weather (user-edited column)".into();
        metadata.model = format!("Custom column, based on {}", origin.model);
        metadata.source_levels = samples.len();
        metadata.extrapolated_refractivity = samples.iter().any(|s| {
            !(233.15..=373.15).contains(&s.temperature_k)
                || !(80000.0..=120000.0).contains(&s.pressure_pa)
        });
        metadata.vertical_input_description = "User-edited temperature, pressure and specific humidity at fixed source heights; linear refractivity interpolation; vacuum at 120 km. Provider metadata describe the original seed, not live custom conditions.".into();
        let mut profile = build_profile(&samples, metadata)?;
        // An edit changes the air, not its already-bound surface/scene frame.
        profile.base_height_m = self.source.base_height_m;
        profile.anchor = self.source.anchor;
        Ok(profile)
    }

    fn parsed_samples(&self) -> Result<Vec<WeatherSample>, AtmosphereError> {
        if self.rows.len() != self.samples.len() {
            return Err(invalid("The custom column must retain all source levels."));
        }
        self.rows.iter().zip(&self.displayed).zip(&self.samples).enumerate()
            .map(|(index, ((row, shown), original))| {
                let mut sample = *original;
                if row.height_label != shown.height_label {
                    return Err(invalid("Source heights are read-only."));
                }
                if row.temperature != shown.temperature {
                    let value = number(&row.temperature, index, "temperature")?;
                    sample.temperature_k = temperature_kelvin(value, self.units);
                }
                if row.pressure_hpa != shown.pressure_hpa {
                    sample.pressure_pa = number(&row.pressure_hpa, index, "pressure")? * 100.0;
                }
                if row.specific_humidity_g_kg != shown.specific_humidity_g_kg {
                    sample.specific_humidity = number(&row.specific_humidity_g_kg, index, "specific humidity")? / 1000.0;
                }
                if !sample.temperature_k.is_finite() || !(100.0..=400.0).contains(&sample.temperature_k) {
                    return Err(invalid(format!("Level {}: temperature must be within 100–400 K (−173.15–126.85 °C).", index+1)));
                }
                if !sample.pressure_pa.is_finite() || !(0.0..=120000.0).contains(&sample.pressure_pa) {
                    return Err(invalid(format!("Level {}: pressure must be within 0–1200 hPa.", index+1)));
                }
                if !sample.specific_humidity.is_finite() || !(0.0..=0.1).contains(&sample.specific_humidity) {
                    return Err(invalid(format!("Level {}: specific humidity must be within 0–100 g/kg (not relative humidity).", index+1)));
                }
                Ok(sample)
            }).collect()
    }
}

fn invalid(message: impl Into<String>) -> AtmosphereError {
    AtmosphereError::InvalidWeather(message.into())
}

fn number(text: &str, index: usize, field: &str) -> Result<f64, AtmosphereError> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| {
            invalid(format!(
                "Level {}: enter a finite number for {field}.",
                index + 1
            ))
        })
}

fn temperature_kelvin(value: f64, units: UnitSystem) -> f64 {
    // The declared decimal display endpoints represent exactly 100/400 K.
    // Map only those exact input values, not nearby out-of-domain numbers:
    // ordinary floating addition can put -173.15 C just below 100 K.
    let (low, high) = match units {
        UnitSystem::Metric => (-173.15, 126.85),
        UnitSystem::Imperial => (-279.67, 260.33),
    };
    if value == low {
        return 100.0;
    }
    if value == high {
        return 400.0;
    }
    match units {
        UnitSystem::Metric => value + 273.15,
        UnitSystem::Imperial => (value - 32.0) / 1.8 + 273.15,
    }
}

fn display_rows(samples: &[WeatherSample], units: UnitSystem) -> Vec<CustomWeatherRow> {
    samples
        .iter()
        .map(|sample| {
            let celsius = sample.temperature_k - 273.15;
            let temperature = match units {
                UnitSystem::Metric => celsius,
                UnitSystem::Imperial => celsius * 1.8 + 32.0,
            };
            CustomWeatherRow {
                height_label: units.format_height(sample.height_m),
                temperature: temperature.to_string(),
                pressure_hpa: (sample.pressure_pa / 100.0).to_string(),
                specific_humidity_g_kg: (sample.specific_humidity * 1000.0).to_string(),
            }
        })
        .collect()
}
