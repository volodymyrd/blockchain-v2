use crate::commands;
use blockchain_net_utils::VALIDATOR_PORT_RANGE_STR;
use clap::{crate_description, crate_name, crate_version, Command};
use solana_hash::Hash;
use std::str::FromStr;

pub struct DefaultArgs {
    pub bind_address: &'static str,
    pub dynamic_port_range: &'static str,
    pub ledger_path: &'static str,
}

impl DefaultArgs {
    pub fn new() -> Self {
        Self {
            bind_address: "0.0.0.0",
            ledger_path: "ledger",
            dynamic_port_range: VALIDATOR_PORT_RANGE_STR,
        }
    }
}

pub fn command(default_args: &DefaultArgs) -> Command {
    let command = Command::new(crate_name!())
        .about(crate_description!())
        .version(crate_version!());

    commands::run::add_args(command, default_args)
}

pub fn parse_port_validator(port: &str) -> Result<u16, String> {
    port.parse::<u16>()
        .map_err(|err| format!("Unable to parse {port}: {err}"))
}

pub(crate) fn parse_hash_validator(hash: &str) -> Result<Hash, String> {
    Hash::from_str(hash).map_err(|e| format!("{e:?}"))
}
