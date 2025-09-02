use blockchain_accounts_db::hardened_unpack::MAX_GENESIS_ARCHIVE_UNPACKED_SIZE;
use blockchain_clap_utils::input_parsers::{
    parse_percentage, parse_pubkey, parse_slot, unix_timestamp_from_rfc3339_datetime,
};
use blockchain_entry::poh::compute_hashes_per_tick;
use blockchain_ledger::blockstore::create_new_ledger;
use blockchain_ledger::blockstore_options::LedgerColumnOptions;
use blockchain_stake_program::{add_genesis_accounts, stake_state};
use blockchain_vote_program::vote_state;
use clap::{crate_description, crate_name, crate_version, Arg, ArgAction, Command};
use solana_account::AccountSharedData;
use solana_clock as clock;
use solana_clock::{Slot, UnixTimestamp};
use solana_cluster_type::ClusterType;
use solana_epoch_schedule::EpochSchedule;
use solana_fee_calculator::FeeRateGovernor;
use solana_genesis_config::GenesisConfig;
use solana_inflation::Inflation;
use solana_native_token::LAMPORTS_PER_SOL;
use solana_poh_config::PohConfig;
use solana_pubkey::Pubkey;
use solana_rent::Rent;
use solana_sdk_ids::system_program;
use solana_stake_interface::state::StakeStateV2;
use solana_vote_interface::state::VoteStateV3;
use std::path::PathBuf;
use std::slice::Iter;
use std::time::Duration;
use std::{io, process};

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
            Arg::new("creation_time")
                .long("creation-time")
                .value_name("RFC3339 DATE TIME")
                .value_parser(unix_timestamp_from_rfc3339_datetime)
                .help("Time when the bootstrap validator will start the cluster [default: current system time]"),
        )
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
            Arg::new("vote_commission_percentage")
                .long("vote-commission-percentage")
                .value_name("NUMBER")
                .default_value("100")
                .help("percentage of vote commission")
                .value_parser(parse_percentage),
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
            Arg::new("primordial_accounts_file")
                .long("primordial-accounts-file")
                .value_name("FILENAME")
                .action(ArgAction::Append)
                .help("The location of pubkey for primordial accounts and balance"),
        )
        .arg(
            Arg::new("validator_accounts_file")
                .long("validator-accounts-file")
                .value_name("FILENAME")
                .action(ArgAction::Append)
                .help("The location of a file containing a list of identity, vote, and \
                stake pubkeys and balances for validator accounts to bake into genesis")
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
        .map(|pubkey| *pubkey)
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
    let faucet_pubkey = matches.try_get_one::<Pubkey>("faucet_pubkey")?.copied();

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

    let commission = matches
        .try_get_one::<u8>("vote_commission_percentage")?
        .copied()
        .unwrap();
    let rent = genesis_config.rent.clone();

    add_validator_accounts(
        &mut genesis_config,
        &mut bootstrap_validator_pubkeys.iter(),
        bootstrap_validator_lamports,
        bootstrap_validator_stake_lamports,
        commission,
        &rent,
        bootstrap_stake_authorized_pubkey.as_ref(),
    )?;

    if let Some(creation_time) = matches
        .try_get_one::<UnixTimestamp>("creation_time")?
        .copied()
    {
        genesis_config.creation_time = creation_time;
    }

    if let Some(faucet_pubkey) = faucet_pubkey {
        genesis_config.add_account(
            faucet_pubkey,
            AccountSharedData::new(faucet_lamports, 0, &system_program::id()),
        );
    }

    add_genesis_accounts(&mut genesis_config);
    // genesis_utils::activate_all_features(&mut genesis_config);
    // if !features_to_deactivate.is_empty() {
    //     solana_runtime::genesis_utils::deactivate_features(
    //         &mut genesis_config,
    //         &features_to_deactivate,
    //     );
    // }

    // if let Some(files) = matches.try_get_many::<&str>("primordial_accounts_file")? {
    //     for file in files {
    //         load_genesis_accounts(file, &mut genesis_config)?;
    //     }
    // }
    //
    // if let Some(files) = matches.try_get_many::<&str>("validator_accounts_file") {
    //     for file in files {
    //         load_validator_accounts(file, commission, &rent, &mut genesis_config)?;
    //     }
    // }

    let max_genesis_archive_unpacked_size = matches
        .try_get_one::<u64>("max_genesis_archive_unpacked_size")?
        .copied()
        .unwrap();

    let issued_lamports = genesis_config
        .accounts
        .values()
        .map(|account| account.lamports)
        .sum::<u64>();

    // skip for development clusters
    // add_genesis_accounts(&mut genesis_config, issued_lamports - faucet_lamports);

    // let parse_address = |address: &str, input_type: &str| {
    //     address.parse::<Pubkey>().unwrap_or_else(|err| {
    //         eprintln!("Error: invalid {input_type} {address}: {err}");
    //         process::exit(1);
    //     })
    // };
    //
    // let parse_program_data = |program: &str| {
    //     let mut program_data = vec![];
    //     File::open(program)
    //         .and_then(|mut file| file.read_to_end(&mut program_data))
    //         .unwrap_or_else(|err| {
    //             eprintln!("Error: failed to read {program}: {err}");
    //             process::exit(1);
    //         });
    //     program_data
    // };
    //
    // if let Some(values) = matches.values_of("bpf_program") {
    //     for (address, loader, program) in values.tuples() {
    //         let address = parse_address(address, "address");
    //         let loader = parse_address(loader, "loader");
    //         let program_data = parse_program_data(program);
    //         genesis_config.add_account(
    //             address,
    //             AccountSharedData::from(Account {
    //                 lamports: genesis_config.rent.minimum_balance(program_data.len()),
    //                 data: program_data,
    //                 executable: true,
    //                 owner: loader,
    //                 rent_epoch: 0,
    //             }),
    //         );
    //     }
    // }
    //
    // if let Some(values) = matches.values_of("upgradeable_program") {
    //     for (address, loader, program, upgrade_authority) in values.tuples() {
    //         let address = parse_address(address, "address");
    //         let loader = parse_address(loader, "loader");
    //         let program_data_elf = parse_program_data(program);
    //         let upgrade_authority_address = if upgrade_authority == "none" {
    //             Pubkey::default()
    //         } else {
    //             upgrade_authority.parse::<Pubkey>().unwrap_or_else(|_| {
    //                 read_keypair_file(upgrade_authority)
    //                     .map(|keypair| keypair.pubkey())
    //                     .unwrap_or_else(|err| {
    //                         eprintln!(
    //                             "Error: invalid upgrade_authority {upgrade_authority}: {err}"
    //                         );
    //                         process::exit(1);
    //                     })
    //             })
    //         };
    //
    //         let (programdata_address, _) =
    //             Pubkey::find_program_address(&[address.as_ref()], &loader);
    //         let mut program_data = bincode::serialize(&UpgradeableLoaderState::ProgramData {
    //             slot: 0,
    //             upgrade_authority_address: Some(upgrade_authority_address),
    //         })
    //             .unwrap();
    //         program_data.extend_from_slice(&program_data_elf);
    //         genesis_config.add_account(
    //             programdata_address,
    //             AccountSharedData::from(Account {
    //                 lamports: genesis_config.rent.minimum_balance(program_data.len()),
    //                 data: program_data,
    //                 owner: loader,
    //                 executable: false,
    //                 rent_epoch: 0,
    //             }),
    //         );
    //
    //         let program_data = bincode::serialize(&UpgradeableLoaderState::Program {
    //             programdata_address,
    //         })
    //             .unwrap();
    //         genesis_config.add_account(
    //             address,
    //             AccountSharedData::from(Account {
    //                 lamports: genesis_config.rent.minimum_balance(program_data.len()),
    //                 data: program_data,
    //                 owner: loader,
    //                 executable: true,
    //                 rent_epoch: 0,
    //             }),
    //         );
    //     }
    // }

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

fn add_validator_accounts(
    genesis_config: &mut GenesisConfig,
    pubkeys_iter: &mut Iter<Pubkey>,
    lamports: u64,
    stake_lamports: u64,
    commission: u8,
    rent: &Rent,
    authorized_pubkey: Option<&Pubkey>,
) -> io::Result<()> {
    rent_exempt_check(
        stake_lamports,
        rent.minimum_balance(StakeStateV2::size_of()),
    )?;

    loop {
        let Some(identity_pubkey) = pubkeys_iter.next() else {
            break;
        };
        let vote_pubkey = pubkeys_iter.next().unwrap();
        let stake_pubkey = pubkeys_iter.next().unwrap();

        genesis_config.add_account(
            *identity_pubkey,
            AccountSharedData::new(lamports, 0, &system_program::id()),
        );

        let vote_account = vote_state::create_account_with_authorized(
            identity_pubkey,
            identity_pubkey,
            identity_pubkey,
            commission,
            VoteStateV3::get_rent_exempt_reserve(rent).max(1),
        );

        genesis_config.add_account(
            *stake_pubkey,
            stake_state::create_account(
                authorized_pubkey.unwrap_or(identity_pubkey),
                vote_pubkey,
                &vote_account,
                rent,
                stake_lamports,
            ),
        );
        genesis_config.add_account(*vote_pubkey, vote_account);
    }
    Ok(())
}

fn rent_exempt_check(stake_lamports: u64, exempt: u64) -> io::Result<()> {
    if stake_lamports < exempt {
        Err(io::Error::other(
            format!(
                "error: insufficient validator stake lamports: {stake_lamports} for rent exemption, requires {exempt}"
            ),
        ))
    } else {
        Ok(())
    }
}
