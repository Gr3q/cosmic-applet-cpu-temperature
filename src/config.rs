use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};

#[derive(Debug, Clone, CosmicConfigEntry, PartialEq, Eq)]
#[version = 1]
pub struct CPUTempAppletConfig {
    pub fahrenheit: bool,
    pub refresh_period_milliseconds: u64,
}

impl Default for CPUTempAppletConfig {
    fn default() -> Self {
        Self {
            fahrenheit: false,
            refresh_period_milliseconds: 1000,
        }
    }
}
