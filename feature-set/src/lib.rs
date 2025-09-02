use ahash::{AHashMap, AHashSet};
use solana_pubkey::Pubkey;
use std::sync::LazyLock;

#[cfg_attr(feature = "frozen-abi", derive(solana_frozen_abi_macro::AbiExample))]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FeatureSet {
    active: AHashMap<Pubkey, u64>,
    inactive: AHashSet<Pubkey>,
}

impl Default for FeatureSet {
    fn default() -> Self {
        Self {
            // All features disabled
            active: AHashMap::new(),
            inactive: AHashSet::from_iter((*FEATURE_NAMES).keys().cloned()),
        }
    }
}

impl FeatureSet {
    pub fn new(active: AHashMap<Pubkey, u64>, inactive: AHashSet<Pubkey>) -> Self {
        Self { active, inactive }
    }

    pub fn active(&self) -> &AHashMap<Pubkey, u64> {
        &self.active
    }

    pub fn active_mut(&mut self) -> &mut AHashMap<Pubkey, u64> {
        &mut self.active
    }

    pub fn inactive(&self) -> &AHashSet<Pubkey> {
        &self.inactive
    }
}

pub mod alpenglow {
    solana_pubkey::declare_id!("mustRekeyVm2QHYB3JPefBiU4BY3Z6JkW2k3Scw5GWP");
}

pub mod secp256k1_program_enabled {
    solana_pubkey::declare_id!("E3PHP7w8kB7np3CTQ1qQ2tW3KCtjRSXBQgW9vM2mWv2Y");
}

pub static FEATURE_NAMES: LazyLock<AHashMap<Pubkey, &'static str>> = LazyLock::new(|| {
    [
        (secp256k1_program_enabled::id(), "secp256k1 program"),
        /*************** ADD NEW FEATURES HERE ***************/
    ]
    .iter()
    .cloned()
    .collect()
});
