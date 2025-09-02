use solana_genesis_config::GenesisConfig;

pub mod stake_state;
pub mod config;
mod epoch_rewards;

pub fn add_genesis_accounts(genesis_config: &mut GenesisConfig) -> u64 {
    let config_lamports = config::add_genesis_account(genesis_config);
    let rewards_lamports = epoch_rewards::add_genesis_account(genesis_config);
    config_lamports.saturating_add(rewards_lamports)
}
