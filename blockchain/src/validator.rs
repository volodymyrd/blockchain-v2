use crate::banking_trace::TraceError;
use anyhow::{anyhow, Result};
use blockchain_accounts_db::hardened_unpack::{open_genesis_config, OpenGenesisConfigError};
use blockchain_gossip::node::Node;
use blockchain_ledger::blockstore::error::BlockstoreError;
use log::info;
use solana_clock::Slot;
use solana_epoch_schedule::MAX_LEADER_SCHEDULE_EPOCH_OFFSET;
use solana_genesis_config::GenesisConfig;
use solana_hash::Hash;
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;

/// Represents a validator in the blockchain network.
///
/// A validator is a node that participates in the consensus process,
/// responsible for verifying transactions and creating new blocks.
pub struct Validator {}

impl Validator {
    pub fn new(
        node: Node,
        identity_keypair: Arc<Keypair>,
        ledger_path: &Path,
        vote_account: &Pubkey,
        config: &ValidatorConfig,
    ) -> Result<Self> {
        let _start_time = Instant::now();

        let id = identity_keypair.pubkey();
        assert_eq!(&id, node.info.pubkey());

        info!("identity pubkey: {id}");
        info!("vote account pubkey: {vote_account}");

        //let mut bank_notification_senders = Vec::new();

        let _exit = Arc::new(AtomicBool::new(false));

        //info!("Initializing sigverify...");

        if !ledger_path.is_dir() {
            return Err(anyhow!(
                "ledger directory does not exist or is not accessible: {ledger_path:?}"
            ));
        }

        let _genesis_config = load_genesis(config, ledger_path)?;

        Ok(Self {})
    }
}

/// Configuration for a validator node.
///
/// This structure holds settings that control the behavior of a validator.
pub struct ValidatorConfig {
    pub expected_genesis_hash: Option<Hash>,

    /// When set to `true`, the validator will not vote on blocks.
    ///
    /// This is useful for running a non-voting node that still keeps track of the chain,
    /// but does not participate in consensus.
    pub voting_disabled: bool,

    pub rpc_addrs: Option<(SocketAddr, SocketAddr)>,

    /// When set to `true`, the validator will require a tower for voting.
    ///
    /// This refers to the Tower BFT consensus algorithm, a data structure that helps a
    /// validator to vote on the correct fork and avoid slashing.
    /// Requiring a tower enhances the security of the validator.
    pub require_tower: bool,

    pub max_genesis_archive_unpacked_size: u64,
}

fn load_genesis(
    config: &ValidatorConfig,
    ledger_path: &Path,
) -> Result<GenesisConfig, ValidatorError> {
    let genesis_config = open_genesis_config(ledger_path, config.max_genesis_archive_unpacked_size)
        .map_err(ValidatorError::OpenGenesisConfig)?;

    // This needs to be limited otherwise the state in the VoteAccount data
    // grows too large
    let leader_schedule_slot_offset = genesis_config.epoch_schedule.leader_schedule_slot_offset;
    let slots_per_epoch = genesis_config.epoch_schedule.slots_per_epoch;
    let leader_epoch_offset = leader_schedule_slot_offset.div_ceil(slots_per_epoch);
    assert!(leader_epoch_offset <= MAX_LEADER_SCHEDULE_EPOCH_OFFSET);

    let genesis_hash = genesis_config.hash();
    info!("genesis hash: {genesis_hash}");

    if let Some(expected_genesis_hash) = config.expected_genesis_hash {
        if genesis_hash != expected_genesis_hash {
            return Err(ValidatorError::GenesisHashMismatch(
                genesis_hash,
                expected_genesis_hash,
            ));
        }
    }

    Ok(genesis_config)
}

#[derive(Error, Debug)]
pub enum ValidatorError {
    #[error("bank hash mismatch: actual={0}, expected={1}")]
    BankHashMismatch(Hash, Hash),

    #[error("blockstore error: {0}")]
    Blockstore(#[source] BlockstoreError),

    #[error("genesis hash mismatch: actual={0}, expected={1}")]
    GenesisHashMismatch(Hash, Hash),

    #[error(
        "ledger does not have enough data to wait for supermajority: current slot={0}, needed \
         slot={1}"
    )]
    NotEnoughLedgerData(Slot, Slot),

    #[error("failed to open genesis: {0}")]
    OpenGenesisConfig(#[source] OpenGenesisConfigError),

    #[error("{0}")]
    Other(String),

    #[error(
        "PoH hashes/second rate is slower than the cluster target: mine {mine}, cluster {target}"
    )]
    PohTooSlow { mine: u64, target: u64 },

    #[error("shred version mismatch: actual {actual}, expected {expected}")]
    ShredVersionMismatch { actual: u16, expected: u16 },

    #[error(transparent)]
    TraceError(#[from] TraceError),

    #[error("Wen Restart finished, please continue with --wait-for-supermajority")]
    WenRestartFinished,
}
