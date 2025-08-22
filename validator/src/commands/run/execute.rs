use crate::commands::{run::args::RunArgs, FromClapArgMatches};
use blockchain_accounts_db::hardened_unpack::MAX_GENESIS_ARCHIVE_UNPACKED_SIZE;
use blockchain_accounts_db::utils::create_and_canonicalize_directory;
use blockchain_core::validator::{Validator, ValidatorConfig};
use blockchain_gossip::cluster_info::{BindIpAddrs, NodeConfig};
use blockchain_gossip::node::Node;
use blockchain_net_utils::{find_available_port_in_range, parse_host, PortRange};
use clap::ArgMatches;
use log::{error, info, warn};
use solana_hash::Hash;
use solana_keypair::Keypair;
use solana_logger::{redirect_stderr_to_file, setup_with_default};
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use std::error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::{Arc, RwLock};

pub const DEFAULT_FILTER: &str = "solana=info,agave=info,blockchain=debug";

pub fn execute(matches: &ArgMatches, ledger_path: &Path) -> Result<(), Box<dyn error::Error>> {
    let run_args = RunArgs::from_clap_arg_match(matches)?;

    let identity_keypair = run_args.identity_keypair;

    let logfile = run_args.logfile;
    let logfile = if logfile == "-" {
        None
    } else {
        println!("log file: {logfile}");
        Some(logfile)
    };
    let _logger_thread = redirect_stderr_to_file(logfile);
    setup_with_default(DEFAULT_FILTER);

    info!("Starting validator with: {:#?}", std::env::args_os());

    let authorized_voter_keypairs = match matches.try_get_one::<Vec<Arc<Keypair>>>("matches") {
        Ok(Some(keypairs)) => keypairs.clone(),
        Err(_) | Ok(None) => {
            vec![identity_keypair.clone()]
        }
    };
    // allow it to be safely shared and potentially modified by multiple threads.
    let _authorized_voter_keypairs = Arc::new(RwLock::new(authorized_voter_keypairs));

    let private_rpc = matches.get_flag("private_rpc");

    // Canonicalize ledger path to avoid issues with symlink creation
    let ledger_path = create_and_canonicalize_directory(ledger_path).map_err(|err| {
        format!(
            "unable to access ledger path '{}': {err}",
            ledger_path.display(),
        )
    })?;

    let _entrypoint_addrs = run_args.entrypoints;

    let bind_addresses = {
        let parsed = matches
            .get_many::<IpAddr>("bind_address")
            .expect("bind_address should always be present due to default")
            .cloned()
            .collect();
        BindIpAddrs::new(parsed).map_err(|err| format!("invalid bind_addresses: {err}"))?
    };

    let rpc_bind_address = if matches.contains_id("rpc_bind_address") {
        *matches
            .get_one::<IpAddr>("rpc_bind_address")
            .expect("invalid rpc_bind_address")
    } else if private_rpc {
        parse_host("127.0.0.1")?
    } else {
        bind_addresses.primary()
    };

    let mut validator_config = ValidatorConfig {
        require_tower: matches.get_flag("require_tower"),
        max_genesis_archive_unpacked_size: MAX_GENESIS_ARCHIVE_UNPACKED_SIZE,
        expected_genesis_hash: matches
            .try_get_one::<Hash>("expected_genesis_hash")?
            .copied(),
        voting_disabled: matches.get_flag("no_voting"),
        rpc_addrs: matches.try_get_one::<u16>("rpc_port")?.map(|rpc_port| {
            (
                SocketAddr::new(rpc_bind_address, *rpc_port),
                SocketAddr::new(rpc_bind_address, *rpc_port + 1),
            )
        }),
    };

    let vote_account = match matches.try_get_one::<Arc<Pubkey>>("vote_account") {
        Ok(Some(pubkey)) => pubkey.clone(),
        Err(e) => {
            if !validator_config.voting_disabled {
                error!("--vote-account could not be parsed {e}, validator will not vote");
                validator_config.voting_disabled = true;
            }
            Arc::new(Keypair::new().pubkey())
        }
        Ok(None) => {
            if !validator_config.voting_disabled {
                warn!("--vote-account not specified, validator will not vote");
                validator_config.voting_disabled = true;
            }
            Arc::new(Keypair::new().pubkey())
        }
    };

    let dynamic_port_range = matches
        .try_get_one::<PortRange>("dynamic_port_range")
        .expect("invalid dynamic_port_range")
        .copied()
        .unwrap();

    let advertised_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);

    let gossip_port = match matches.try_get_one::<u16>("gossip_port")? {
        None => find_available_port_in_range(bind_addresses.primary(), (0, 1))?,
        Some(&port) => port,
    };

    let node_config = NodeConfig {
        advertised_ip,
        gossip_port,
        port_range: dynamic_port_range,
        bind_ip_addrs: bind_addresses,
    };

    let node = Node::new_with_external_ip(&identity_keypair.pubkey(), node_config);

    let _validator = match Validator::new(
        node,
        identity_keypair,
        &ledger_path,
        &vote_account,
        &validator_config,
    ) {
        Ok(validator) => Ok(validator),
        Err(err) => Err(format!("{err:?}")),
    }?;

    info!("Validator initialized");
    //validator.join();
    info!("Validator exiting..");

    Ok(())
}
