use crate::blockstore::column::Column;
use crate::blockstore_db::{IteratorMode, LedgerColumn, Rocks};
use crate::blockstore_meta::TransactionStatusIndexMeta;
use crate::blockstore_metrics::BlockstoreRpcApiMetrics;
use crate::blockstore_options::{
    BlockstoreOptions, LedgerColumnOptions, BLOCKSTORE_DIRECTORY_ROCKS_LEVEL,
};
use crate::slot_stats::SlotsStats;
use bincode::deserialize;
use blockchain_entry::entry::create_ticks;
use blockchain_measure::measure::Measure;
use column::columns as cf;
use crossbeam_channel::{Receiver, Sender};
use log::info;
use solana_clock::Slot;
use solana_genesis_config::GenesisConfig;
use solana_hash::Hash;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, RwLock};
pub use {
    crate::blockstore::error::{BlockstoreError, Result},
    rocksdb::properties as RocksProperties,
};

pub mod column;
pub mod error;

// Creates a new ledger with slot 0 full of ticks (and only ticks).
//
// Returns the blockhash that can be used to append entries with.
pub fn create_new_ledger(
    ledger_path: &Path,
    genesis_config: &GenesisConfig,
    max_genesis_archive_unpacked_size: u64,
    column_options: LedgerColumnOptions,
) -> Result<Hash> {
    Blockstore::destroy(ledger_path)?;
    genesis_config.write(ledger_path)?;

    // Fill slot 0 with ticks that link back to the genesis_config to bootstrap the ledger.
    let blockstore_dir = BLOCKSTORE_DIRECTORY_ROCKS_LEVEL;
    let blockstore = Blockstore::open_with_options(
        ledger_path,
        BlockstoreOptions {
            enforce_ulimit_nofile: false,
            column_options: column_options.clone(),
            ..BlockstoreOptions::default()
        },
    )?;
    let ticks_per_slot = genesis_config.ticks_per_slot;
    let hashes_per_tick = genesis_config.poh_config.hashes_per_tick.unwrap_or(0);
    let entries = create_ticks(ticks_per_slot, hashes_per_tick, genesis_config.hash());
    let last_hash = entries.last().unwrap().hash;

    //let shredder = Shredder::new(0, 0, 0, version).unwrap();

    Ok(last_hash)
}

pub type CompletedSlotsSender = Sender<Vec<Slot>>;
pub type CompletedSlotsReceiver = Receiver<Vec<Slot>>;

// ledger window
pub struct Blockstore {
    ledger_path: PathBuf,
    db: Arc<Rocks>,
    // Column families
    address_signatures_cf: LedgerColumn<cf::AddressSignatures>,
    bank_hash_cf: LedgerColumn<cf::BankHash>,
    block_height_cf: LedgerColumn<cf::BlockHeight>,
    blocktime_cf: LedgerColumn<cf::Blocktime>,
    code_shred_cf: LedgerColumn<cf::ShredCode>,
    data_shred_cf: LedgerColumn<cf::ShredData>,
    dead_slots_cf: LedgerColumn<cf::DeadSlots>,
    erasure_meta_cf: LedgerColumn<cf::ErasureMeta>,
    index_cf: LedgerColumn<cf::Index>,
    merkle_root_meta_cf: LedgerColumn<cf::MerkleRootMeta>,
    meta_cf: LedgerColumn<cf::SlotMeta>,
    optimistic_slots_cf: LedgerColumn<cf::OptimisticSlots>,
    orphans_cf: LedgerColumn<cf::Orphans>,
    perf_samples_cf: LedgerColumn<cf::PerfSamples>,
    rewards_cf: LedgerColumn<cf::Rewards>,
    roots_cf: LedgerColumn<cf::Root>,
    transaction_memos_cf: LedgerColumn<cf::TransactionMemos>,
    transaction_status_cf: LedgerColumn<cf::TransactionStatus>,
    transaction_status_index_cf: LedgerColumn<cf::TransactionStatusIndex>,

    highest_primary_index_slot: RwLock<Option<Slot>>,
    max_root: AtomicU64,
    insert_shreds_lock: Mutex<()>,
    new_shreds_signals: Mutex<Vec<Sender<bool>>>,
    completed_slots_senders: Mutex<Vec<CompletedSlotsSender>>,
    pub lowest_cleanup_slot: RwLock<Slot>,
    pub slots_stats: SlotsStats,
    rpc_api_metrics: BlockstoreRpcApiMetrics,
}

impl Blockstore {
    /// Opens a Ledger in directory, provides "infinite" window of shreds
    pub fn open(ledger_path: &Path) -> Result<Blockstore> {
        Self::do_open(ledger_path, BlockstoreOptions::default())
    }

    pub fn open_with_options(ledger_path: &Path, options: BlockstoreOptions) -> Result<Blockstore> {
        Self::do_open(ledger_path, options)
    }

    /// Deletes the blockstore at the specified path.
    ///
    /// Note that if the `ledger_path` has multiple rocksdb instances, this
    /// function will destroy all.
    pub fn destroy(ledger_path: &Path) -> Result<()> {
        // Database::destroy() fails if the root directory doesn't exist
        fs::create_dir_all(ledger_path)?;
        Rocks::destroy(&Path::new(ledger_path).join(BLOCKSTORE_DIRECTORY_ROCKS_LEVEL))
    }

    fn do_open(ledger_path: &Path, options: BlockstoreOptions) -> Result<Blockstore> {
        fs::create_dir_all(ledger_path)?;
        let blockstore_path = ledger_path.join(BLOCKSTORE_DIRECTORY_ROCKS_LEVEL);

        //adjust_ulimit_nofile(options.enforce_ulimit_nofile)?;

        // Open the database
        let mut measure = Measure::start("blockstore open");
        info!("Opening blockstore at {blockstore_path:?}");
        let db = Arc::new(Rocks::open(blockstore_path, options)?);

        let address_signatures_cf = db.column();
        let bank_hash_cf = db.column();
        let block_height_cf = db.column();
        let blocktime_cf = db.column();
        let code_shred_cf = db.column();
        let data_shred_cf = db.column();
        let dead_slots_cf = db.column();
        let erasure_meta_cf = db.column();
        let index_cf = db.column();
        let merkle_root_meta_cf = db.column();
        let meta_cf = db.column();
        let optimistic_slots_cf = db.column();
        let orphans_cf = db.column();
        let perf_samples_cf = db.column();
        let rewards_cf = db.column();
        let roots_cf = db.column();
        let transaction_memos_cf = db.column();
        let transaction_status_cf = db.column();
        let transaction_status_index_cf = db.column();

        // Get max root or 0 if it doesn't exist
        let max_root = roots_cf
            .iter(IteratorMode::End)?
            .next()
            .map(|(slot, _)| slot)
            .unwrap_or(0);
        let max_root = AtomicU64::new(max_root);

        measure.stop();
        info!("Opening blockstore done; {measure}");
        let blockstore = Blockstore {
            ledger_path: ledger_path.to_path_buf(),
            db,
            address_signatures_cf,
            bank_hash_cf,
            block_height_cf,
            blocktime_cf,
            code_shred_cf,
            data_shred_cf,
            dead_slots_cf,
            erasure_meta_cf,
            index_cf,
            merkle_root_meta_cf,
            meta_cf,
            optimistic_slots_cf,
            orphans_cf,
            perf_samples_cf,
            rewards_cf,
            roots_cf,
            transaction_memos_cf,
            transaction_status_cf,
            transaction_status_index_cf,
            highest_primary_index_slot: RwLock::<Option<Slot>>::default(),
            new_shreds_signals: Mutex::default(),
            completed_slots_senders: Mutex::default(),
            insert_shreds_lock: Mutex::<()>::default(),
            max_root,
            lowest_cleanup_slot: RwLock::<Slot>::default(),
            slots_stats: SlotsStats::default(),
            rpc_api_metrics: BlockstoreRpcApiMetrics::default(),
        };
        blockstore.cleanup_old_entries()?;
        blockstore.update_highest_primary_index_slot()?;

        Ok(blockstore)
    }

    /// Returns whether the blockstore has primary (read and write) access
    pub fn is_primary_access(&self) -> bool {
        self.db.is_primary_access()
    }

    fn cleanup_old_entries(&self) -> Result<()> {
        if !self.is_primary_access() {
            return Ok(());
        }

        // Initialize TransactionStatusIndexMeta if they are not present already
        if self.transaction_status_index_cf.get(0)?.is_none() {
            self.transaction_status_index_cf
                .put(0, &TransactionStatusIndexMeta::default())?;
        }
        if self.transaction_status_index_cf.get(1)?.is_none() {
            self.transaction_status_index_cf
                .put(1, &TransactionStatusIndexMeta::default())?;
        }

        let address_signatures_dummy_key = cf::AddressSignatures::as_index(2);
        if self
            .address_signatures_cf
            .get(address_signatures_dummy_key)?
            .is_some()
        {
            self.address_signatures_cf
                .delete(address_signatures_dummy_key)?;
        };

        Ok(())
    }

    fn update_highest_primary_index_slot(&self) -> Result<()> {
        let iterator = self.transaction_status_index_cf.iter(IteratorMode::Start)?;
        let mut highest_primary_index_slot = None;
        for (_, data) in iterator {
            let meta: TransactionStatusIndexMeta = deserialize(&data).unwrap();
            if highest_primary_index_slot.is_none()
                || highest_primary_index_slot.is_some_and(|slot| slot < meta.max_slot)
            {
                highest_primary_index_slot = Some(meta.max_slot);
            }
        }
        if highest_primary_index_slot.is_some_and(|slot| slot != 0) {
            self.set_highest_primary_index_slot(highest_primary_index_slot);
        } else {
            self.db.set_clean_slot_0(true);
        }
        Ok(())
    }

    fn set_highest_primary_index_slot(&self, slot: Option<Slot>) {
        *self.highest_primary_index_slot.write().unwrap() = slot;
    }
}
