use crate::cli::{command, DefaultArgs};
use std::error;
use std::path::PathBuf;

mod bootstrap;
mod cli;
mod commands;

fn main() -> Result<(), Box<dyn error::Error>> {
    let default_args = DefaultArgs::new();
    let matches = command(&default_args)
        .try_get_matches()
        .unwrap_or_else(|e| e.exit());

    let ledger_path = PathBuf::from(matches.try_get_one::<String>("ledger_path")?.unwrap());

    commands::run::execute(&matches, &ledger_path)
}
