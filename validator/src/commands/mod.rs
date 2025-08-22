pub mod run;

use clap::parser::MatchesError;
use std::fmt::Debug;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Clap(#[from] MatchesError),

    #[error(transparent)]
    Dynamic(#[from] Box<dyn std::error::Error>),
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait FromClapArgMatches {
    fn from_clap_arg_match(matches: &clap::ArgMatches) -> Result<Self>
    where
        Self: Sized;
}
