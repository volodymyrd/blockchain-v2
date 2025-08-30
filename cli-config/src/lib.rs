use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io;

/// The CLI configuration.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// The default signing source, which may be a keypair file, but may also
    /// represent several other types of signers, as described in the
    /// documentation for `solana_clap_utils::keypair::signer_from_path`.
    /// Because it represents sources other than a simple path, the name
    /// `keypair_path` is misleading, and exists for backwards compatibility
    /// reasons.
    ///
    /// The signing source can be loaded with either the `signer_from_path`
    /// function, or with `solana_clap_utils::keypair::DefaultSigner`.
    pub keypair_path: String,
}

impl Default for Config {
    fn default() -> Self {
        let keypair_path = {
            let mut keypair_path = dirs_next::home_dir().expect("home directory");
            keypair_path.extend([".config", "solana", "id.json"]);
            keypair_path.to_str().unwrap().to_string()
        };

        Self { keypair_path }
    }
}

impl Config {
    /// Loads a configuration from file.
    pub fn load(config_file: &str) -> Result<Self, io::Error> {
        let file = File::open(config_file)?;
        let config =
            serde_yaml_ng::from_reader(file).map_err(|err| io::Error::other(format!("{err:?}")))?;
        Ok(config)
    }
}
