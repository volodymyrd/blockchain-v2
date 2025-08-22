use log::warn;
use solana_accounts_db::hardened_unpack::{unpack_genesis_archive, UnpackError};
use solana_genesis_config::{GenesisConfig, DEFAULT_GENESIS_ARCHIVE};
use std::path::Path;
use thiserror::Error;

pub const MAX_GENESIS_ARCHIVE_UNPACKED_SIZE: u64 = 10 * 1024 * 1024;

#[derive(Error, Debug)]
pub enum OpenGenesisConfigError {
    #[error("unpack error: {0}")]
    Unpack(#[from] UnpackError),
    #[error("Genesis load error: {0}")]
    Load(#[from] std::io::Error),
}

pub fn open_genesis_config(
    ledger_path: &Path,
    max_genesis_archive_unpacked_size: u64,
) -> std::result::Result<GenesisConfig, OpenGenesisConfigError> {
    match GenesisConfig::load(ledger_path) {
        Ok(genesis_config) => Ok(genesis_config),
        Err(load_err) => {
            warn!(
                "Failed to load genesis_config at {ledger_path:?}: {load_err}. Will attempt to \
                 unpack genesis archive and then retry loading."
            );

            let genesis_package = ledger_path.join(DEFAULT_GENESIS_ARCHIVE);
            unpack_genesis_archive(
                &genesis_package,
                ledger_path,
                max_genesis_archive_unpacked_size,
            )?;
            GenesisConfig::load(ledger_path).map_err(OpenGenesisConfigError::Load)
        }
    }
}
