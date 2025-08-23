mod keypair;
mod mnemonic;

use crate::mnemonic::{
    acquire_passphrase_and_message, language_arg, no_passphrase_arg, try_get_language,
    try_get_word_count, word_count_arg,
};
use bip39::{Mnemonic, MnemonicType, Seed};
use blockchain_cli_config::Config;
use clap::{crate_description, crate_name, crate_version, Arg, ArgAction, ArgMatches, Command};
use solana_keypair::{keypair_from_seed, write_keypair, write_keypair_file, Keypair};
use solana_signer::Signer;
use std::error;
use std::path::Path;

const CONFIG_FILE: &str = "config_file";

fn main() -> Result<(), Box<dyn error::Error>> {
    let matches = Command::new(crate_name!())
        .about(crate_description!())
        .version(crate_version!())
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            Arg::new(CONFIG_FILE)
                .short('C')
                .long("config")
                .value_name("FILEPATH")
                .help("Configuration file to use"),
        )
        .subcommand(
            Command::new("new")
                .about("Generate new keypair file from a random seed phrase")
                .arg(
                    Arg::new("outfile")
                        .short('o')
                        .long("outfile")
                        .value_name("FILEPATH")
                        .help("Path to generated file"),
                )
                .arg(
                    Arg::new("force")
                        .short('f')
                        .long("force")
                        .action(ArgAction::SetTrue)
                        .help("Overwrite the output file if it exists"),
                )
                .arg(
                    Arg::new("silent")
                        .short('s')
                        .long("silent")
                        .action(ArgAction::SetTrue)
                        .help("Do not display seed phrase."),
                )
                .key_generation_common_args(),
        )
        .try_get_matches()
        .unwrap_or_else(|e| e.exit());

    let _ = if let Some(config_file) = matches.try_get_one::<String>(CONFIG_FILE)? {
        Config::load(config_file)?
    } else {
        Config::default()
    };

    if let Some(subcommand) = matches.subcommand() {
        match subcommand {
            ("new", matches) => {
                let mut path = std::env::home_dir().expect("home directory");
                let outfile = if matches.try_contains_id("outfile")? {
                    matches.get_one::<String>("outfile").map(|s| s.as_str())
                } else if matches.try_contains_id(NO_OUTFILE_ARG.name)? {
                    None
                } else {
                    path.extend([".config", "blockchain", "id.json"]);
                    Some(path.to_str().unwrap())
                };
                let word_count = try_get_word_count(matches)?.unwrap();
                let language = try_get_language(matches)?.unwrap();

                let silent = matches.get_flag("silent");
                if !silent {
                    println!("Generating a new keypair");
                }

                let mnemonic_type = MnemonicType::for_word_count(word_count)?;
                let mnemonic = Mnemonic::new(mnemonic_type, language);
                let (passphrase, passphrase_message) = acquire_passphrase_and_message(matches)
                    .map_err(|err| format!("Unable to acquire passphrase: {err}"))?;
                let seed = Seed::new(&mnemonic, &passphrase);
                let keypair = keypair_from_seed(seed.as_bytes())?;

                if let Some(outfile) = outfile {
                    check_for_overwrite(outfile, matches)?;
                    output_keypair(&keypair, outfile, "new")
                        .map_err(|err| format!("Unable to write {outfile}: {err}"))?;
                }

                if !silent {
                    let phrase: &str = mnemonic.phrase();
                    let divider = String::from_utf8(vec![b'='; phrase.len()]).unwrap();
                    println!(
                        "{}\npubkey: {}\n{}\nSave this seed phrase{} to recover your new keypair:\n{}\n{}",
                        &divider, keypair.pubkey(), &divider, passphrase_message, phrase, &divider
                    );
                }
            }
            _ => unreachable!(),
        }
    }

    Ok(())
}

// Sentinel value used to indicate to write to screen instead of file
pub const STDOUT_OUTFILE_TOKEN: &str = "-";

fn output_keypair(
    keypair: &Keypair,
    outfile: &str,
    source: &str,
) -> Result<(), Box<dyn error::Error>> {
    if outfile == STDOUT_OUTFILE_TOKEN {
        let mut stdout = std::io::stdout();
        write_keypair(keypair, &mut stdout)?;
    } else {
        write_keypair_file(keypair, outfile)?;
        println!("Wrote {source} keypair to {outfile}");
    }
    Ok(())
}

pub(crate) struct ArgConstant<'a> {
    pub long: &'a str,
    pub name: &'a str,
    pub help: &'a str,
}

const NO_OUTFILE_ARG: ArgConstant<'static> = ArgConstant {
    long: "no-outfile",
    name: "no_outfile",
    help: "Only print a seed phrase and pubkey. Do not output a keypair file",
};

trait KeyGenerationCommonArgs {
    fn key_generation_common_args(self) -> Self;
}

impl KeyGenerationCommonArgs for Command {
    fn key_generation_common_args(self) -> Self {
        self.arg(word_count_arg())
            .arg(language_arg())
            .arg(no_passphrase_arg())
    }
}

pub fn check_for_overwrite(
    outfile: &str,
    matches: &ArgMatches,
) -> Result<(), Box<dyn error::Error>> {
    let force = matches.get_flag("force");
    if !force && Path::new(outfile).exists() {
        let err_msg = format!("Refusing to overwrite {outfile} without --force flag");
        return Err(err_msg.into());
    }
    Ok(())
}
