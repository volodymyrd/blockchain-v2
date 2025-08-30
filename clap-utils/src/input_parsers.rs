use solana_clock::Slot;
use solana_keypair::{read_keypair_file, Keypair};
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;

pub fn parse_keypair_from_path(path: &str) -> Result<Arc<Keypair>, String> {
    read_keypair_file(path)
        .map(Arc::new)
        .map_err(|e| format!("failed to read keypair file '{path}': {e}"))
}

pub fn parse_pubkey_from_path(path: &str) -> Result<Pubkey, String> {
    read_keypair_file(path)
        .map(|keypair| keypair.pubkey())
        .map_err(|e| format!("failed to read keypair file '{path}': {e}"))
}

pub fn parse_percentage(percentage: &str) -> Result<u8, String> {
    percentage
        .parse::<u8>()
        .map_err(|e| format!("Unable to parse input percentage, provided: {percentage}, err: {e}"))
        .and_then(|v| {
            if v > 100 {
                Err(format!(
                    "Percentage must be in range of 0 to 100, provided: {v}"
                ))
            } else {
                Ok(v)
            }
        })
}

pub fn parse_slot(slot: &str) -> Result<Slot, String> {
    parse_generic::<Slot, _>(slot)
}

pub fn parse_pubkey(pubkey: &str) -> Result<Pubkey, String> {
    parse_generic::<Pubkey, _>(pubkey).or_else(|_| parse_pubkey_from_path(pubkey))
}

fn parse_generic<U, T>(string: T) -> Result<U, String>
where
    T: AsRef<str> + Display,
    U: FromStr,
    U::Err: Display,
{
    string
        .as_ref()
        .parse::<U>()
        .map_err(|err| format!("error parsing '{string}': {err}"))
}
