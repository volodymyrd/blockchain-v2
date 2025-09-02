use bincode::serialize;
use solana_account::{Account, AccountSharedData, ReadableAccount, WritableAccount};
use solana_config_interface::state::ConfigKeys;
use solana_genesis_config::GenesisConfig;
use solana_pubkey::Pubkey;
use solana_stake_interface::config::Config;

#[allow(deprecated)]
pub fn add_genesis_account(genesis_config: &mut GenesisConfig) -> u64 {
    let mut account = create_config_account(vec![], &Config::default(), 0);
    let lamports = std::cmp::max(genesis_config.rent.minimum_balance(account.data().len()), 1);

    account.set_lamports(lamports);

    genesis_config.add_account(solana_stake_interface::config::id(), account);

    lamports
}

#[allow(deprecated)]
fn create_config_account(
    keys: Vec<(Pubkey, bool)>,
    config_data: &Config,
    lamports: u64,
) -> AccountSharedData {
    let mut data = serialize(&ConfigKeys { keys }).unwrap();
    data.extend_from_slice(&serialize(config_data).unwrap());
    AccountSharedData::from(Account {
        lamports,
        data,
        owner: solana_sdk_ids::config::id(),
        ..Account::default()
    })
}
