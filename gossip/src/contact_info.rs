use serde::Serialize;
use solana_pubkey::Pubkey;
use solana_serde_varint as serde_varint;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ContactInfo {
    pubkey: Pubkey,
    #[serde(with = "serde_varint")]
    wallclock: u64,
}

impl ContactInfo {
    pub fn new(pubkey: Pubkey, wallclock: u64) -> Self {
        Self { pubkey, wallclock }
    }

    #[inline]
    pub fn pubkey(&self) -> &Pubkey {
        &self.pubkey
    }
}
