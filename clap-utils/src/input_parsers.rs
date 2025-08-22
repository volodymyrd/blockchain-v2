use solana_keypair::{read_keypair_file, Keypair};
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use std::sync::Arc;

pub fn parse_keypair_from_path(path: &str) -> Result<Arc<Keypair>, String> {
    read_keypair_file(path)
        .map(Arc::new)
        .map_err(|e| format!("failed to read keypair file '{path}': {e}"))
}

pub fn parse_pubkey_from_path(path: &str) -> Result<Arc<Pubkey>, String> {
    read_keypair_file(path)
        .map(|keypair| keypair.pubkey())
        .map(Arc::new)
        .map_err(|e| format!("failed to read keypair file '{path}': {e}"))
}
