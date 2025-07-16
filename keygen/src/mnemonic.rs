use crate::keypair::prompt_passphrase;
use crate::ArgConstant;
use bip39::Language;
use clap::builder::PossibleValuesParser;
use clap::{Arg, ArgAction, ArgMatches};
use std::error;

pub(crate) const NO_PASSPHRASE: &str = "";

pub(crate) const WORD_COUNT_ARG: ArgConstant<'static> = ArgConstant {
    long: "word-count",
    name: "word_count",
    help: "Specify the number of words that will be present in the generated seed phrase",
};

pub(crate) const LANGUAGE_ARG: ArgConstant<'static> = ArgConstant {
    long: "language",
    name: "language",
    help: "Specify the mnemonic language that will be present in the generated seed phrase",
};

pub(crate) const NO_PASSPHRASE_ARG: ArgConstant<'static> = ArgConstant {
    long: "no-bip39-passphrase",
    name: "no_passphrase",
    help: "Do not prompt for a BIP39 passphrase",
};

const POSSIBLE_WORD_COUNTS: &[&str] = &["12", "24"];

pub(crate) fn word_count_arg() -> Arg {
    Arg::new(WORD_COUNT_ARG.name)
        .long(WORD_COUNT_ARG.long)
        .value_parser(PossibleValuesParser::new(POSSIBLE_WORD_COUNTS))
        .default_value("12")
        .value_name("NUMBER")
        .help(WORD_COUNT_ARG.help)
}

pub(crate) fn try_get_word_count(
    matches: &ArgMatches,
) -> Result<Option<usize>, Box<dyn error::Error>> {
    Ok(matches
        .try_get_one::<String>(WORD_COUNT_ARG.name)?
        .map(|count| match count.as_str() {
            "12" => 12,
            "24" => 24,
            _ => unreachable!(),
        }))
}

const POSSIBLE_LANGUAGES: &[&str] = &[
    "english",
    "chinese-simplified",
    "chinese-traditional",
    "japanese",
    "spanish",
    "korean",
    "french",
    "italian",
];

pub(crate) fn language_arg() -> Arg {
    Arg::new(LANGUAGE_ARG.name)
        .long(LANGUAGE_ARG.long)
        .value_parser(PossibleValuesParser::new(POSSIBLE_LANGUAGES))
        .default_value("english")
        .value_name("LANGUAGE")
        .help(LANGUAGE_ARG.help)
}

pub(crate) fn try_get_language(
    matches: &ArgMatches,
) -> Result<Option<Language>, Box<dyn error::Error>> {
    Ok(matches
        .try_get_one::<String>(LANGUAGE_ARG.name)?
        .map(|language| match language.as_str() {
            "english" => Language::English,
            "chinese-simplified" => Language::ChineseSimplified,
            "chinese-traditional" => Language::ChineseTraditional,
            "japanese" => Language::Japanese,
            "spanish" => Language::Spanish,
            "korean" => Language::Korean,
            "french" => Language::French,
            "italian" => Language::Italian,
            _ => unreachable!(),
        }))
}

pub(crate) fn no_passphrase_arg() -> Arg {
    Arg::new(NO_PASSPHRASE_ARG.name)
        .long(NO_PASSPHRASE_ARG.long)
        .alias("no-passphrase")
        .help(NO_PASSPHRASE_ARG.help)
        .action(ArgAction::SetTrue)
}

pub(crate) fn acquire_passphrase_and_message(
    matches: &ArgMatches,
) -> Result<(String, String), Box<dyn error::Error>> {
    if matches.try_contains_id(NO_PASSPHRASE_ARG.name)? {
        Ok(no_passphrase_and_message())
    } else {
        match prompt_passphrase(
            "\nFor added security, enter a BIP39 passphrase\n\
             \nNOTE! This passphrase improves security of the recovery seed phrase NOT the\n\
             keypair file itself, which is stored as insecure plain text\n\
             \nBIP39 Passphrase (empty for none): ",
        ) {
            Ok(passphrase) => {
                println!();
                Ok((passphrase, " and your BIP39 passphrase".to_string()))
            }
            Err(e) => Err(e),
        }
    }
}

pub(crate) fn no_passphrase_and_message() -> (String, String) {
    (NO_PASSPHRASE.to_string(), "".to_string())
}
