use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io;

/// The CLI configuration.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {}

impl Config {
    /// Loads a configuration from file.
    pub fn load(config_file: &str) -> Result<Self, io::Error> {
        let file = File::open(config_file)?;
        let config =
            serde_yaml_ng::from_reader(file).map_err(|err| io::Error::other(format!("{err:?}")))?;
        Ok(config)
    }
}
