use blockchain_feature_set::{alpenglow, FeatureSet};
use solana_account::Account;
use solana_feature_gate_interface::{self as feature, Feature};
use solana_genesis_config::GenesisConfig;
use solana_pubkey::Pubkey;

pub fn activate_all_features(genesis_config: &mut GenesisConfig) {
    do_activate_all_features::<false>(genesis_config);
}

fn do_activate_all_features<const IS_ALPENGLOW: bool>(genesis_config: &mut GenesisConfig) {
    // Activate all features at genesis in development mode
    for feature_id in FeatureSet::default().inactive() {
        if IS_ALPENGLOW || *feature_id != alpenglow::id() {
            activate_feature(genesis_config, *feature_id);
        }
    }
}

pub fn activate_feature(genesis_config: &mut GenesisConfig, feature_id: Pubkey) {
    genesis_config.accounts.insert(
        feature_id,
        Account::from(feature::create_account(
            &Feature {
                activated_at: Some(0),
            },
            std::cmp::max(genesis_config.rent.minimum_balance(Feature::size_of()), 1),
        )),
    );
}
