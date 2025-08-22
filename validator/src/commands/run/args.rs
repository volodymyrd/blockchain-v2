use crate::cli::{parse_hash_validator, parse_port_validator, DefaultArgs};
use crate::commands::FromClapArgMatches;
use blockchain_clap_utils::input_parsers::{parse_keypair_from_path, parse_pubkey_from_path};
use blockchain_net_utils::{parse_host, parse_host_port, parse_port_range};
use clap::{Arg, ArgAction, ArgMatches, Command};
use solana_keypair::Keypair;
use solana_signer::Signer;
use std::net::SocketAddr;
use std::sync::Arc;

pub fn add_args(command: Command, default_args: &DefaultArgs) -> Command {
    command
        .arg(
            Arg::new("identity")
                .short('i')
                .long("identity")
                .value_name("KEYPAIR")
                .value_parser(parse_keypair_from_path)
                .help("Validator identity keypair"),
        )
        // The authorized_voter_keypairs argument provides a crucial security feature
        // by separating a validator's main identity from the key it uses for the high-frequency
        // task of signing votes.•Hot vs. Cold Keys: It allows you to use a "hot" key for
        // voting (--authorized-voter) that must be online, while keeping the validator's
        // primary "cold" identity key (--identity), which is tied to its stake,
        // in a more secure, offline location.•Damage Control: If the online voting key is
        // ever compromised, an attacker cannot steal the validator's stake.
        // The operator can simply generate a new voting keypair and authorize it
        // for the vote account, while the main identity key remains secure.In short,
        // it's a security best practice that significantly reduces the risk of running a
        // validator by minimizing the exposure of the most critical key.
        .arg(
            Arg::new("authorized_voter_keypairs")
                .long("authorized-voter")
                .value_name("KEYPAIR")
                .value_parser(parse_keypair_from_path)
                .action(ArgAction::Append)
                .requires("vote_account")
                .help(
                    "Include an additional authorized voter keypair. May be specified multiple \
                 times. [default: the --identity keypair]",
                ),
        )
        .arg(
            Arg::new("vote_account")
                .long("vote-account")
                .value_name("ADDRESS")
                .value_parser(parse_pubkey_from_path)
                .requires("identity")
                .help(
                    "Validator vote account public key. If unspecified, voting will be disabled. \
                 The authorized voter for the account must either be the --identity keypair \
                 or set by the --authorized-voter argument",
                ),
        )
        .arg(
            Arg::new("ledger_path")
                .short('l')
                .long("ledger")
                .value_name("DIR")
                .default_value(default_args.ledger_path)
                .help("Use DIR as ledger location"),
        )
        .arg(
            Arg::new("entrypoint")
                .short('n')
                .long("entrypoint")
                .value_name("HOST:PORT")
                .value_parser(parse_host_port)
                .action(ArgAction::Append)
                .help("Rendezvous with the cluster at this gossip entrypoint"),
        )
        .arg(
            Arg::new("no_voting")
                .long("no-voting")
                .action(ArgAction::SetTrue)
                .help("Launch validator without voting"),
        )
        .arg(
            Arg::new("rpc_port")
                .long("rpc-port")
                .value_name("PORT")
                .value_parser(parse_port_validator)
                .help("Enable JSON RPC on this port, and the next port for the RPC websocket"),
        )
        .arg(
            Arg::new("private_rpc")
                .long("private-rpc")
                .action(ArgAction::SetTrue)
                .help("Do not publish the RPC port for use by others"),
        )
        .arg(
            Arg::new("dynamic_port_range")
                .long("dynamic-port-range")
                .value_name("MIN_PORT-MAX_PORT")
                .default_value(default_args.dynamic_port_range)
                .value_parser(parse_port_range)
                .help("Range to use for dynamically assigned ports"),
        )
        .arg(
            Arg::new("require_tower")
                .long("require-tower")
                .action(ArgAction::SetTrue)
                .help("Refuse to start if saved tower state is not found"),
        )
        .arg(
            Arg::new("expected_genesis_hash")
                .long("expected-genesis-hash")
                .value_name("HASH")
                .value_parser(parse_hash_validator)
                .help("Require the genesis have this hash"),
        )
        .arg(
            Arg::new("bind_address")
                .long("bind-address")
                .value_name("HOST")
                .value_parser(parse_host)
                .default_value(default_args.bind_address)
                .action(ArgAction::Append)
                .help(
                    "Repeatable. IP addresses to bind the validator ports on. \
                First is primary (used on startup), the rest may be switched to during operation.",
                ),
        )
        .arg(
            Arg::new("rpc_bind_address")
                .long("rpc-bind-address")
                .value_name("HOST")
                .value_parser(parse_host)
                .help(
                    "IP address to bind the RPC port [default: 127.0.0.1 if --private-rpc is \
                 present, otherwise use --bind-address]",
                ),
        )
        .arg(
            Arg::new("gossip_port")
                .long("gossip-port")
                .value_name("PORT")
                .value_parser(parse_port_validator)
                .help("Gossip port number for the validator"),
        )
        .arg(
            Arg::new("logfile")
                .short('o')
                .long("log")
                .value_name("FILE")
                .help(
                    "Redirect logging to the specified file, '-' for standard error. Sending the \
                 SIGUSR1 signal to the validator process will cause it to re-open the log file",
                ),
        )
}
#[derive(Debug, PartialEq)]
pub struct RunArgs {
    pub identity_keypair: Arc<Keypair>,
    pub logfile: String,
    pub entrypoints: Vec<SocketAddr>,
    // pub known_validators: Option<HashSet<Pubkey>>,
    // pub socket_addr_space: SocketAddrSpace,
    // pub rpc_bootstrap_config: RpcBootstrapConfig,
    // pub blockstore_options: BlockstoreOptions,
}

impl FromClapArgMatches for RunArgs {
    fn from_clap_arg_match(matches: &ArgMatches) -> crate::commands::Result<Self>
    where
        Self: Sized,
    {
        let identity_keypair = matches
            .try_get_one::<Arc<Keypair>>("identity")?
            .ok_or_else(|| {
                crate::commands::Error::Dynamic(
                    "Validator identity keypair is required (--identity)".into(),
                )
            })?;

        let logfile = matches
            .get_one::<String>("logfile")
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("agave-validator-{}.log", identity_keypair.pubkey()));

        let mut entrypoints: Vec<SocketAddr> = matches
            .get_many::<SocketAddr>("entrypoint")
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        // sort() + dedup() to yield a vector of unique elements
        entrypoints.sort();
        entrypoints.dedup();

        // let known_validators = validators_set(
        //     &identity_keypair.pubkey(),
        //     matches,
        //     "known_validators",
        //     "known validator",
        // )?;
        //
        // let socket_addr_space = SocketAddrSpace::new(matches.is_present("allow_private_addr"));

        Ok(RunArgs {
            identity_keypair: identity_keypair.clone(),
            logfile,
            entrypoints,
            // known_validators,
            // socket_addr_space,
            // rpc_bootstrap_config: RpcBootstrapConfig::from_clap_arg_match(matches)?,
            // blockstore_options: BlockstoreOptions::from_clap_arg_match(matches)?,
        })
    }
}
