use blockchain_accounts_db::hardened_unpack::MAX_GENESIS_ARCHIVE_UNPACKED_SIZE;
use blockchain_clap_utils::input_parsers::{parse_percentage, parse_pubkey, parse_slot};
use blockchain_entry::poh::compute_hashes_per_tick;
use blockchain_ledger::blockstore::create_new_ledger;
use blockchain_ledger::blockstore_options::LedgerColumnOptions;
use clap::{crate_description, crate_name, crate_version, Arg, ArgAction, Command};
use solana_clock as clock;
use solana_clock::Slot;
use solana_cluster_type::ClusterType;
use solana_epoch_schedule::EpochSchedule;
use solana_fee_calculator::FeeRateGovernor;
use solana_genesis_config::GenesisConfig;
use solana_inflation::Inflation;
use solana_native_token::LAMPORTS_PER_SOL;
use solana_poh_config::PohConfig;
use solana_pubkey::Pubkey;
use solana_rent::Rent;
use solana_stake_interface::state::StakeStateV2;
use solana_vote_interface::state::VoteStateV3;
use std::path::PathBuf;
use std::process;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let default_faucet_pubkey = Box::leak(
        blockchain_cli_config::Config::default()
            .keypair_path
            .into_boxed_str(),
    ) as &'static str;
    let (
        default_target_lamports_per_signature,
        default_target_signatures_per_slot,
        default_fee_burn_percentage,
    ) = {
        let fee_rate_governor = FeeRateGovernor::default();
        (
            Box::leak(
                fee_rate_governor
                    .target_lamports_per_signature
                    .to_string()
                    .into_boxed_str(),
            ) as &'static str,
            Box::leak(
                fee_rate_governor
                    .target_signatures_per_slot
                    .to_string()
                    .into_boxed_str(),
            ) as &'static str,
            Box::leak(fee_rate_governor.burn_percent.to_string().into_boxed_str()) as &'static str,
        )
    };

    let rent = Rent::default();
    let (
        default_lamports_per_byte_year,
        default_rent_exemption_threshold,
        default_rent_burn_percentage,
    ) = {
        (
            Box::leak(rent.lamports_per_byte_year.to_string().into_boxed_str()) as &'static str,
            Box::leak(rent.exemption_threshold.to_string().into_boxed_str()) as &'static str,
            Box::leak(rent.burn_percent.to_string().into_boxed_str()) as &'static str,
        )
    };

    // vote account
    let default_bootstrap_validator_lamports = Box::leak(
        (500 * LAMPORTS_PER_SOL)
            .max(VoteStateV3::get_rent_exempt_reserve(&rent))
            .to_string()
            .into_boxed_str(),
    ) as &'static str;
    // stake account
    let default_bootstrap_validator_stake_lamports = Box::leak(
        (LAMPORTS_PER_SOL / 2)
            .max(rent.minimum_balance(StakeStateV2::size_of()))
            .to_string()
            .into_boxed_str(),
    ) as &'static str;

    let default_target_tick_duration = PohConfig::default().target_tick_duration;
    let default_ticks_per_slot =
        Box::leak(clock::DEFAULT_TICKS_PER_SLOT.to_string().into_boxed_str()) as &'static str;
    let default_cluster_type = "mainnet-beta";
    let default_genesis_archive_unpacked_size = Box::leak(
        MAX_GENESIS_ARCHIVE_UNPACKED_SIZE
            .to_string()
            .into_boxed_str(),
    ) as &'static str;

    let matches = Command::new(crate_name!())
        .about(crate_description!())
        .version(crate_version!())
        .arg(
            Arg::new("bootstrap_validator")
                .short('b')
                .long("bootstrap-validator")
                .value_name("IDENTITY_PUBKEY VOTE_PUBKEY STAKE_PUBKEY")
                .value_parser(parse_pubkey)
                .number_of_values(3)
                .action(ArgAction::Append)
                .required(true)
                .help("The bootstrap validator's identity, vote and stake pubkeys"),
        )
        .arg(
            Arg::new("ledger_path")
                .short('l')
                .long("ledger")
                .value_name("DIR")
                .required(true)
                .help("Use directory as persistent ledger location"),
        )
        .arg(
            Arg::new("faucet_lamports")
                .short('t')
                .long("faucet-lamports")
                .value_name("LAMPORTS")
                .value_parser(clap::value_parser!(u64))
                .help("Number of lamports to assign to the faucet"),
        )
        .arg(
            Arg::new("faucet_pubkey")
                .short('m')
                .long("faucet-pubkey")
                .value_name("PUBKEY")
                .value_parser(parse_pubkey)
                .requires("faucet_lamports")
                .default_value(default_faucet_pubkey)
                .help("Path to file containing the faucet's pubkey"),
        )
        .arg(
            Arg::new("bootstrap_stake_authorized_pubkey")
                .long("bootstrap-stake-authorized-pubkey")
                .value_name("BOOTSTRAP STAKE AUTHORIZED PUBKEY")
                .value_parser(parse_pubkey)
                .help(
                    "Path to file containing the pubkey authorized to manage the bootstrap \
                     validator's stake [default: --bootstrap-validator IDENTITY_PUBKEY]",
                ),
        )
        .arg(
            Arg::new("bootstrap_validator_lamports")
                .long("bootstrap-validator-lamports")
                .value_name("LAMPORTS")
                .default_value(default_bootstrap_validator_lamports)
                .value_parser(clap::value_parser!(u64))
                .help("Number of lamports to assign to the bootstrap validator"),
        )
        .arg(
            Arg::new("bootstrap_validator_stake_lamports")
                .long("bootstrap-validator-stake-lamports")
                .value_name("LAMPORTS")
                .default_value(default_bootstrap_validator_stake_lamports)
                .value_parser(clap::value_parser!(u64))
                .help("Number of lamports to assign to the bootstrap validator's stake account"),
        )
        .arg(
            Arg::new("target_lamports_per_signature")
                .long("target-lamports-per-signature")
                .value_name("LAMPORTS")
                .default_value(default_target_lamports_per_signature)
                .value_parser(clap::value_parser!(u64))
                .help(
                    "The cost in lamports that the cluster will charge for signature \
                     verification when the cluster is operating at target-signatures-per-slot",
                ),
        )
        .arg(
            Arg::new("lamports_per_byte_year")
                .long("lamports-per-byte-year")
                .value_name("LAMPORTS")
                .default_value(default_lamports_per_byte_year)
                .value_parser(clap::value_parser!(u64))
                .help(
                    "The cost in lamports that the cluster will charge per byte per year \
                     for accounts with data",
                ),
        )
        .arg(
            Arg::new("rent_exemption_threshold")
                .long("rent-exemption-threshold")
                .value_name("NUMBER")
                .default_value(default_rent_exemption_threshold)
                .value_parser(clap::value_parser!(f64))
                .help(
                    "amount of time (in years) the balance has to include rent for \
                     to qualify as rent exempted account",
                ),
        )
        .arg(
            Arg::new("rent_burn_percentage")
                .long("rent-burn-percentage")
                .value_name("NUMBER")
                .default_value(default_rent_burn_percentage)
                .help("percentage of collected rent to burn")
                .value_parser(parse_percentage),
        )
        .arg(
            Arg::new("fee_burn_percentage")
                .long("fee-burn-percentage")
                .value_name("NUMBER")
                .default_value(default_fee_burn_percentage)
                .value_parser(parse_percentage)
                .help("percentage of collected fee to burn"),
        )
        .arg(
            Arg::new("target_signatures_per_slot")
                .long("target-signatures-per-slot")
                .value_name("NUMBER")
                .default_value(default_target_signatures_per_slot)
                .value_parser(clap::value_parser!(u64))
                .help(
                    "Used to estimate the desired processing capacity of the cluster. \
                    When the latest slot processes fewer/greater signatures than this \
                    value, the lamports-per-signature fee will decrease/increase for \
                    the next slot. A value of 0 disables signature-based fee adjustments",
                ),
        )
        .arg(
            Arg::new("target_tick_duration")
                .long("target-tick-duration")
                .value_name("MILLIS")
                .value_parser(clap::value_parser!(u64))
                .help("The target tick rate of the cluster in milliseconds"),
        )
        .arg(
            Arg::new("hashes_per_tick")
                .long("hashes-per-tick")
                .value_name("NUM_HASHES|\"auto\"|\"sleep\"")
                .default_value("auto")
                .help(
                    "How many PoH hashes to roll before emitting the next tick. \
                     If \"auto\", determine based on --target-tick-duration \
                     and the hash rate of this computer. If \"sleep\", for development \
                     sleep for --target-tick-duration instead of hashing",
                ),
        )
        .arg(
            Arg::new("ticks_per_slot")
                .long("ticks-per-slot")
                .value_name("TICKS")
                .default_value(default_ticks_per_slot)
                .value_parser(clap::value_parser!(u64))
                .help("The number of ticks in a slot"),
        )
        .arg(
            Arg::new("slots_per_epoch")
                .long("slots-per-epoch")
                .value_name("SLOTS")
                .value_parser(parse_slot)
                .help("The number of slots in an epoch"),
        )
        .arg(
            Arg::new("enable_warmup_epochs")
                .long("enable-warmup-epochs")
                .action(ArgAction::SetTrue)
                .help(
                    "When enabled epochs start short and will grow. \
                     Useful for warming up stake quickly during development",
                ),
        )
        .arg(
            Arg::new("cluster_type")
                .long("cluster-type")
                .value_parser(clap::value_parser!(ClusterType))
                .default_value(default_cluster_type)
                .help("Selects the features that will be enabled for the cluster"),
        )
        .arg(
            Arg::new("max_genesis_archive_unpacked_size")
                .long("max-genesis-archive-unpacked-size")
                .value_name("NUMBER")
                .default_value(default_genesis_archive_unpacked_size)
                .value_parser(clap::value_parser!(u64))
                .help("maximum total uncompressed file size of created genesis archive"),
        )
        .arg(
            Arg::new("inflation")
                .long("inflation")
                .value_parser(["pico", "full", "none"])
                .help("Selects inflation"),
        )
        .try_get_matches()
        .unwrap_or_else(|e| {
            eprintln!("failed to parse args: {}", e);
            e.exit()
        });

    let ledger_path = PathBuf::from(matches.try_get_one::<String>("ledger_path")?.unwrap());

    let rent = Rent {
        lamports_per_byte_year: matches
            .try_get_one::<u64>("lamports_per_byte_year")?
            .copied()
            .unwrap(),
        exemption_threshold: matches
            .try_get_one::<f64>("rent_exemption_threshold")?
            .copied()
            .unwrap(),
        burn_percent: matches
            .try_get_one::<u8>("rent_burn_percentage")?
            .copied()
            .unwrap(),
    };

    // can use unwrap as the param is required.
    let bootstrap_validator_pubkeys = matches
        .try_get_many::<Pubkey>("bootstrap_validator")?
        .unwrap()
        .into_iter()
        .collect::<Vec<_>>();
    assert_eq!(bootstrap_validator_pubkeys.len() % 3, 0);

    // Ensure there are no duplicated pubkeys in the --bootstrap-validator list
    {
        let mut v = bootstrap_validator_pubkeys.clone();
        v.sort();
        v.dedup();
        if v.len() != bootstrap_validator_pubkeys.len() {
            eprintln!("Error: --bootstrap-validator pubkeys cannot be duplicated");
            process::exit(1);
        }
    }

    let bootstrap_validator_lamports = matches
        .try_get_one::<u64>("bootstrap_validator_lamports")?
        .copied()
        .unwrap();

    let bootstrap_validator_stake_lamports = matches
        .try_get_one::<u64>("bootstrap_validator_stake_lamports")?
        .copied()
        .unwrap();

    let bootstrap_stake_authorized_pubkey = matches
        .try_get_one::<Pubkey>("bootstrap_stake_authorized_pubkey")?
        .copied();
    let faucet_lamports = matches
        .try_get_one::<u64>("faucet_lamports")?
        .copied()
        .unwrap_or(0);
    let faucet_pubkey = matches
        .try_get_one::<Pubkey>("faucet_pubkey")?
        .copied()
        .unwrap();

    // can use unwrap as we provided a default value.
    let ticks_per_slot = matches
        .try_get_one::<u64>("ticks_per_slot")?
        .copied()
        .unwrap();

    let mut fee_rate_governor = FeeRateGovernor::new(
        matches
            .try_get_one::<u64>("target_lamports_per_signature")?
            .copied()
            .unwrap(),
        matches
            .try_get_one::<u64>("target_signatures_per_slot")?
            .copied()
            .unwrap(),
    );
    fee_rate_governor.burn_percent = matches
        .try_get_one::<u8>("fee_burn_percentage")?
        .copied()
        .unwrap();

    let mut poh_config = PohConfig {
        target_tick_duration: match matches.try_get_one::<u64>("target_tick_duration")? {
            None => default_target_tick_duration,
            Some(&tick) => Duration::from_micros(tick),
        },
        ..PohConfig::default()
    };

    let cluster_type = matches
        .try_get_one::<ClusterType>("cluster_type")?
        .copied()
        .unwrap();

    // Get the features to deactivate if provided
    // let features_to_deactivate = features_to_deactivate_for_cluster(&cluster_type, &matches)
    //     .unwrap_or_else(|e| {
    //         eprintln!("{e}");
    //         std::process::exit(1);
    //     });

    match matches
        .try_get_one::<String>("hashes_per_tick")?
        .unwrap()
        .as_str()
    {
        "auto" => match cluster_type {
            ClusterType::Development => {
                let hashes_per_tick =
                    compute_hashes_per_tick(poh_config.target_tick_duration, 1_000_000);
                poh_config.hashes_per_tick = Some(hashes_per_tick / 2); // use 50% of peak ability
            }
            ClusterType::Devnet | ClusterType::Testnet | ClusterType::MainnetBeta => {
                poh_config.hashes_per_tick = Some(clock::DEFAULT_HASHES_PER_TICK);
            }
        },
        "sleep" => {
            poh_config.hashes_per_tick = None;
        }
        s => {
            poh_config.hashes_per_tick = Some(s.parse::<u64>().unwrap_or_else(|err| {
                eprintln!("Error: invalid value for --hashes-per-tick: {s}: {err}");
                process::exit(1);
            }));
        }
    }

    let slots_per_epoch = match matches.try_get_one::<Slot>("slots_per_epoch")? {
        None => match cluster_type {
            ClusterType::Development => clock::DEFAULT_DEV_SLOTS_PER_EPOCH,
            ClusterType::Devnet | ClusterType::Testnet | ClusterType::MainnetBeta => {
                clock::DEFAULT_SLOTS_PER_EPOCH
            }
        },
        Some(slot) => *slot,
    };
    let epoch_schedule = EpochSchedule::custom(
        slots_per_epoch,
        slots_per_epoch,
        matches.get_flag("enable_warmup_epochs"),
    );

    let mut genesis_config = GenesisConfig {
        native_instruction_processors: vec![],
        ticks_per_slot,
        poh_config,
        fee_rate_governor,
        rent,
        epoch_schedule,
        cluster_type,
        ..GenesisConfig::default()
    };

    if let Some(raw_inflation) = matches.get_one::<String>("inflation") {
        let inflation = match raw_inflation.as_str() {
            "pico" => Inflation::pico(),
            "full" => Inflation::full(),
            "none" => Inflation::new_disabled(),
            _ => unreachable!(),
        };
        genesis_config.inflation = inflation;
    }

    let max_genesis_archive_unpacked_size = matches
        .try_get_one::<u64>("max_genesis_archive_unpacked_size")?
        .copied()
        .unwrap();

    solana_logger::setup();
    create_new_ledger(
        &ledger_path,
        &genesis_config,
        max_genesis_archive_unpacked_size,
        LedgerColumnOptions::default(),
    )?;

    println!("{genesis_config}");
    Ok(())
}
