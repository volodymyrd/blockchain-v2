use crate::blockstore::column::{
    columns, Column, ColumnName, TypedColumn, DEPRECATED_PROGRAM_COSTS_COLUMN_NAME,
};
use crate::blockstore::error::Result;
use crate::blockstore_metrics::{maybe_enable_rocksdb_perf, PerfSamplingStatus};
use crate::blockstore_options::{AccessType, BlockstoreOptions, LedgerColumnOptions};
use log::{info, warn};
use rocksdb::compaction_filter::CompactionFilter;
use rocksdb::compaction_filter_factory::{CompactionFilterContext, CompactionFilterFactory};
pub use rocksdb::Direction as IteratorDirection;
use rocksdb::{
    ColumnFamily, ColumnFamilyDescriptor, CompactionDecision, DBCompressionType, DBIterator,
    DBPinnableSlice, IteratorMode as RocksIteratorMode, Options, DB,
};
use solana_clock::Slot;
use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::fs;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

const MAX_WRITE_BUFFER_SIZE: u64 = 256 * 1024 * 1024; // 256MB

// SST files older than this value will be picked up for compaction. This value
// was chosen to be one day to strike a balance between storage getting
// reclaimed in a timely manner and the additional I/O that compaction incurs.
// For more details on this property, see
// https://github.com/facebook/rocksdb/blob/749b179c041347d150fa6721992ae8398b7d2b39/
//   include/rocksdb/advanced_options.h#L908C30-L908C30
const PERIODIC_COMPACTION_SECONDS: u64 = 60 * 60 * 24;

#[derive(Default, Clone, Debug)]
struct OldestSlot {
    slot: Arc<AtomicU64>,
    clean_slot_0: Arc<AtomicBool>,
}

impl OldestSlot {
    pub(crate) fn set_clean_slot_0(&self, clean_slot_0: bool) {
        self.clean_slot_0.store(clean_slot_0, Ordering::Relaxed);
    }

    pub(crate) fn get_clean_slot_0(&self) -> bool {
        self.clean_slot_0.load(Ordering::Relaxed)
    }

    pub fn get(&self) -> Slot {
        // copy from the AtomicU64 as a general precaution so that the oldest_slot can not mutate
        // across single run of compaction for simpler reasoning although this isn't strict
        // requirement at the moment
        // also eventual propagation (very Relaxed) load is Ok, because compaction by nature doesn't
        // require strictly synchronized semantics in this regard
        self.slot.load(Ordering::Relaxed)
    }
}

pub enum IteratorMode<Index> {
    Start,
    End,
    From(Index, IteratorDirection),
}

#[derive(Debug)]
pub struct LedgerColumn<C: Column + ColumnName> {
    backend: Arc<Rocks>,
    column: PhantomData<C>,
    pub column_options: Arc<LedgerColumnOptions>,
    read_perf_status: PerfSamplingStatus,
    write_perf_status: PerfSamplingStatus,
}

impl<C> LedgerColumn<C>
where
    C: Column + ColumnName,
{
    pub fn iter(
        &self,
        iterator_mode: IteratorMode<C::Index>,
    ) -> Result<impl Iterator<Item = (C::Index, Box<[u8]>)> + '_> {
        let start_key: <C as Column>::Key;
        let iterator_mode = match iterator_mode {
            IteratorMode::Start => RocksIteratorMode::Start,
            IteratorMode::End => RocksIteratorMode::End,
            IteratorMode::From(start, direction) => {
                start_key = <C as Column>::key(&start);
                RocksIteratorMode::From(start_key.as_ref(), direction)
            }
        };

        let iter = self.backend.iterator_cf(self.handle(), iterator_mode);
        Ok(iter.map(|pair| {
            let (key, value) = pair.unwrap();
            (C::index(&key), value)
        }))
    }

    #[inline]
    pub fn handle(&self) -> &ColumnFamily {
        self.backend.cf_handle(C::NAME)
    }

    pub fn delete(&self, index: C::Index) -> Result<()> {
        let key = <C as Column>::key(&index);
        self.backend.delete_cf(self.handle(), key)
    }
}

impl<C> LedgerColumn<C>
where
    C: TypedColumn + ColumnName,
{
    pub fn get(&self, index: C::Index) -> Result<Option<C::Type>> {
        let key = <C as Column>::key(&index);
        self.get_raw(key)
    }

    pub fn put(&self, index: C::Index, value: &C::Type) -> Result<()> {
        let serialized_value = C::serialize(value)?;

        let key = <C as Column>::key(&index);
        let result = self.backend.put_cf(self.handle(), key, &serialized_value);

        result
    }

    pub fn get_raw<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<C::Type>> {
        let mut result = Ok(None);
        if let Some(pinnable_slice) = self.backend.get_pinned_cf(self.handle(), key)? {
            let value = C::deserialize(pinnable_slice.as_ref())?;
            result = Ok(Some(value))
        }
        result
    }
}

#[derive(Debug)]
pub(crate) struct Rocks {
    db: DB,
    path: PathBuf,
    access_type: AccessType,
    oldest_slot: OldestSlot,
    column_options: Arc<LedgerColumnOptions>,
    write_batch_perf_status: PerfSamplingStatus,
}

impl Rocks {
    pub(crate) fn open(path: PathBuf, options: BlockstoreOptions) -> Result<Rocks> {
        let recovery_mode = options.recovery_mode.clone();

        fs::create_dir_all(&path)?;

        // Use default database options
        let mut db_options = get_db_options(&options);
        if let Some(recovery_mode) = recovery_mode {
            db_options.set_wal_recovery_mode(recovery_mode.into());
        }
        let oldest_slot = OldestSlot::default();
        let cf_descriptors = Self::cf_descriptors(&path, &options, &oldest_slot);
        let column_options = Arc::from(options.column_options);

        // Open the database
        let mut db = match options.access_type {
            AccessType::Primary | AccessType::PrimaryForMaintenance => {
                DB::open_cf_descriptors(&db_options, &path, cf_descriptors)?
            }
            AccessType::Secondary => {
                let secondary_path = path.join("solana-secondary");
                info!(
                    "Opening Rocks with secondary (read only) access at: {secondary_path:?}. This \
                     secondary access could temporarily degrade other accesses, such as by \
                     agave-validator"
                );
                DB::open_cf_descriptors_as_secondary(
                    &db_options,
                    &path,
                    &secondary_path,
                    cf_descriptors,
                )?
            }
        };

        // Delete the now unused program_costs column if it is present
        if db.cf_handle(DEPRECATED_PROGRAM_COSTS_COLUMN_NAME).is_some() {
            db.drop_cf(DEPRECATED_PROGRAM_COSTS_COLUMN_NAME)?;
        }

        let rocks = Rocks {
            db,
            path,
            access_type: options.access_type,
            oldest_slot,
            column_options,
            write_batch_perf_status: PerfSamplingStatus::default(),
        };

        rocks.configure_compaction();

        Ok(rocks)
    }

    /// Create the column family (CF) descriptors necessary to open the database.
    ///
    /// In order to open a RocksDB database with Primary access, all columns must be opened. So,
    /// in addition to creating descriptors for all the expected columns, also create
    /// descriptors for columns that were discovered but are otherwise unknown to the software.
    ///
    /// One case where columns could be unknown is if a RocksDB database is modified with a newer
    /// software version that adds a new column, and then also opened with an older version that
    /// did not have knowledge of that new column.
    fn cf_descriptors(
        path: &Path,
        options: &BlockstoreOptions,
        oldest_slot: &OldestSlot,
    ) -> Vec<ColumnFamilyDescriptor> {
        let mut cf_descriptors = vec![
            new_cf_descriptor::<columns::SlotMeta>(options, oldest_slot),
            new_cf_descriptor::<columns::DeadSlots>(options, oldest_slot),
            new_cf_descriptor::<columns::ErasureMeta>(options, oldest_slot),
            new_cf_descriptor::<columns::Orphans>(options, oldest_slot),
            new_cf_descriptor::<columns::BankHash>(options, oldest_slot),
            new_cf_descriptor::<columns::Root>(options, oldest_slot),
            new_cf_descriptor::<columns::Index>(options, oldest_slot),
            new_cf_descriptor::<columns::ShredData>(options, oldest_slot),
            new_cf_descriptor::<columns::ShredCode>(options, oldest_slot),
            new_cf_descriptor::<columns::TransactionStatus>(options, oldest_slot),
            new_cf_descriptor::<columns::AddressSignatures>(options, oldest_slot),
            new_cf_descriptor::<columns::TransactionMemos>(options, oldest_slot),
            new_cf_descriptor::<columns::TransactionStatusIndex>(options, oldest_slot),
            new_cf_descriptor::<columns::Rewards>(options, oldest_slot),
            new_cf_descriptor::<columns::Blocktime>(options, oldest_slot),
            new_cf_descriptor::<columns::PerfSamples>(options, oldest_slot),
            new_cf_descriptor::<columns::BlockHeight>(options, oldest_slot),
            new_cf_descriptor::<columns::OptimisticSlots>(options, oldest_slot),
            new_cf_descriptor::<columns::MerkleRootMeta>(options, oldest_slot),
        ];

        // If the access type is Secondary, we don't need to open all of the
        // columns so we can just return immediately.
        match options.access_type {
            AccessType::Secondary => {
                return cf_descriptors;
            }
            AccessType::Primary | AccessType::PrimaryForMaintenance => {}
        }

        // Attempt to detect the column families that are present. It is not a
        // fatal error if we cannot, for example, if the Blockstore is brand
        // new and will be created by the call to Rocks::open().
        let detected_cfs = match DB::list_cf(&Options::default(), path) {
            Ok(detected_cfs) => detected_cfs,
            Err(err) => {
                warn!("Unable to detect Rocks columns: {err:?}");
                vec![]
            }
        };
        // The default column is handled automatically, we don't need to create
        // a descriptor for it
        const DEFAULT_COLUMN_NAME: &str = "default";
        let known_cfs: HashSet<_> = cf_descriptors
            .iter()
            .map(|cf_descriptor| cf_descriptor.name().to_string())
            .chain(std::iter::once(DEFAULT_COLUMN_NAME.to_string()))
            .collect();
        detected_cfs.iter().for_each(|cf_name| {
            if !known_cfs.contains(cf_name.as_str()) {
                info!("Detected unknown column {cf_name}, opening column with basic options");
                // This version of the software was unaware of the column, so
                // it is fair to assume that we will not attempt to read or
                // write the column. So, set some bare bones settings to avoid
                // using extra resources on this unknown column.
                let mut options = Options::default();
                // Lower the default to avoid unnecessary allocations
                options.set_write_buffer_size(1024 * 1024);
                // Disable compactions to avoid any modifications to the column
                options.set_disable_auto_compactions(true);
                cf_descriptors.push(ColumnFamilyDescriptor::new(cf_name, options));
            }
        });

        cf_descriptors
    }

    const fn columns() -> [&'static str; 19] {
        [
            columns::ErasureMeta::NAME,
            columns::DeadSlots::NAME,
            columns::Index::NAME,
            columns::Orphans::NAME,
            columns::BankHash::NAME,
            columns::Root::NAME,
            columns::SlotMeta::NAME,
            columns::ShredData::NAME,
            columns::ShredCode::NAME,
            columns::TransactionStatus::NAME,
            columns::AddressSignatures::NAME,
            columns::TransactionMemos::NAME,
            columns::TransactionStatusIndex::NAME,
            columns::Rewards::NAME,
            columns::Blocktime::NAME,
            columns::PerfSamples::NAME,
            columns::BlockHeight::NAME,
            columns::OptimisticSlots::NAME,
            columns::MerkleRootMeta::NAME,
        ]
    }

    pub(crate) fn is_primary_access(&self) -> bool {
        self.access_type == AccessType::Primary
            || self.access_type == AccessType::PrimaryForMaintenance
    }

    pub(crate) fn cf_handle(&self, cf: &str) -> &ColumnFamily {
        self.db
            .cf_handle(cf)
            .expect("should never get an unknown column")
    }

    // Configure compaction on a per-column basis
    fn configure_compaction(&self) {
        // If compactions are disabled altogether, no need to tune values
        if should_disable_auto_compactions(&self.access_type) {
            info!(
                "Rocks's automatic compactions are disabled due to {:?} access",
                self.access_type
            );
            return;
        }

        // Some columns make use of rocksdb's compaction to help in cleaning
        // the database. See comments in should_enable_cf_compaction() for more
        // details on why some columns need compaction and why others do not.
        //
        // More specifically, periodic (automatic) compaction is used as
        // opposed to manual compaction requests on a range.
        // - Periodic compaction operates on individual files once the file
        //   has reached a certain (configurable) age. See comments at
        //   PERIODIC_COMPACTION_SECONDS for some more deatil.
        // - Manual compaction operates on a range and could end up propagating
        //   through several files and/or levels of the db.
        //
        // Given that data is inserted into the db at a somewhat steady rate,
        // the age of the individual files will be fairly evently distributed
        // over time as well. Thus, the I/O to perform cleanup with periodic
        // compaction is also evenly distributed over time. On the other hand,
        // a manual compaction spanning a large numbers of files could cause
        // a sudden burst in I/O. Such a burst could potentially cause a write
        // stall in addition to negatively impacting other parts of the system.
        // Thus, the choice to use periodic compactions is fairly easy.
        for cf_name in Self::columns() {
            if should_enable_cf_compaction(cf_name) {
                let cf_handle = self.cf_handle(cf_name);
                self.db
                    .set_options_cf(
                        &cf_handle,
                        &[(
                            "periodic_compaction_seconds",
                            &PERIODIC_COMPACTION_SECONDS.to_string(),
                        )],
                    )
                    .unwrap();
            }
        }
    }

    pub(crate) fn column<C>(self: &Arc<Self>) -> LedgerColumn<C>
    where
        C: Column + ColumnName,
    {
        let column_options = Arc::clone(&self.column_options);
        LedgerColumn {
            backend: Arc::clone(self),
            column: PhantomData,
            column_options,
            read_perf_status: PerfSamplingStatus::default(),
            write_perf_status: PerfSamplingStatus::default(),
        }
    }

    pub(crate) fn destroy(path: &Path) -> Result<()> {
        DB::destroy(&Options::default(), path)?;

        Ok(())
    }

    pub(crate) fn iterator_cf(
        &self,
        cf: &ColumnFamily,
        iterator_mode: RocksIteratorMode,
    ) -> DBIterator {
        self.db.iterator_cf(cf, iterator_mode)
    }

    fn put_cf<K: AsRef<[u8]>>(&self, cf: &ColumnFamily, key: K, value: &[u8]) -> Result<()> {
        self.db.put_cf(cf, key, value)?;
        Ok(())
    }

    fn delete_cf<K: AsRef<[u8]>>(&self, cf: &ColumnFamily, key: K) -> Result<()> {
        self.db.delete_cf(cf, key)?;
        Ok(())
    }

    pub(crate) fn set_clean_slot_0(&self, clean_slot_0: bool) {
        self.oldest_slot.set_clean_slot_0(clean_slot_0);
    }

    fn get_pinned_cf(
        &self,
        cf: &ColumnFamily,
        key: impl AsRef<[u8]>,
    ) -> Result<Option<DBPinnableSlice>> {
        let opt = self.db.get_pinned_cf(cf, key)?;
        Ok(opt)
    }
}

/// The default number of threads to use for rocksdb compaction in the rocksdb
/// low priority threadpool
pub fn default_num_compaction_threads() -> NonZeroUsize {
    NonZeroUsize::new(num_cpus::get()).expect("thread count is non-zero")
}

/// The default number of threads to use for rocksdb memtable flushes in the
/// rocksdb high priority threadpool
pub fn default_num_flush_threads() -> NonZeroUsize {
    NonZeroUsize::new((num_cpus::get() / 4).max(1)).expect("thread count is non-zero")
}

fn new_cf_descriptor<C: 'static + Column + ColumnName>(
    options: &BlockstoreOptions,
    oldest_slot: &OldestSlot,
) -> ColumnFamilyDescriptor {
    ColumnFamilyDescriptor::new(C::NAME, get_cf_options::<C>(options, oldest_slot))
}

fn get_db_options(blockstore_options: &BlockstoreOptions) -> Options {
    let mut options = Options::default();

    // Create missing items to support a clean start
    options.create_if_missing(true);
    options.create_missing_column_families(true);

    // rocksdb builds two threadpools: low and high priority. The low priority
    // pool is used for compactions whereas the high priority pool is used for
    // memtable flushes. Separate pools are created so that compactions are
    // unable to stall memtable flushes (which could stall memtable writes).
    //
    // For now, use the deprecated methods to configure the exact amount of
    // threads for each pool. The new method, set_max_background_jobs(N),
    // configures N/4 low priority threads and 3N/4 high priority threads.
    #[allow(deprecated)]
    {
        options.set_max_background_compactions(
            blockstore_options.num_rocksdb_compaction_threads.get() as i32,
        );
        options
            .set_max_background_flushes(blockstore_options.num_rocksdb_flush_threads.get() as i32);
    }
    // Set max total wal size to 4G.
    options.set_max_total_wal_size(4 * 1024 * 1024 * 1024);

    if should_disable_auto_compactions(&blockstore_options.access_type) {
        options.set_disable_auto_compactions(true);
    }

    // Limit to (10) 50 MB log files (500 MB total)
    // Logs grow at < 5 MB / hour, so this provides several days of logs
    options.set_max_log_file_size(50 * 1024 * 1024);
    options.set_keep_log_file_num(10);

    // Allow Rocks to open/keep open as many files as it needs for performance;
    // however, this is also explicitly required for a secondary instance.
    // See https://github.com/facebook/rocksdb/wiki/Secondary-instance
    options.set_max_open_files(-1);

    options
}

// Returns whether automatic compactions should be disabled for the entire
// database based upon the given access type.
fn should_disable_auto_compactions(access_type: &AccessType) -> bool {
    // Leave automatic compactions enabled (do not disable) in Primary mode;
    // disable in all other modes to prevent accidental cleaning
    !matches!(access_type, AccessType::Primary)
}

// Returns whether compactions should be enabled for the given column (name).
fn should_enable_cf_compaction(cf_name: &str) -> bool {
    // In order to keep the ledger storage footprint within a desired size,
    // LedgerCleanupService removes data in FIFO order by slot.
    //
    // Several columns do not contain slot in their key. These columns must
    // be manually managed to avoid unbounded storage growth.
    //
    // Columns where slot is the primary index can be efficiently cleaned via
    // Database::delete_range_cf() && Database::delete_file_in_range_cf().
    //
    // Columns where a slot is part of the key but not the primary index can
    // not be range deleted like above. Instead, the individual key/value pairs
    // must be iterated over and a decision to keep or discard that pair is
    // made. The comparison logic is implemented in PurgedSlotFilter which is
    // configured to run as part of rocksdb's automatic compactions. Storage
    // space is reclaimed on this class of columns once compaction has
    // completed on a given range or file.
    matches!(
        cf_name,
        columns::TransactionStatus::NAME
            | columns::TransactionMemos::NAME
            | columns::AddressSignatures::NAME
    )
}

fn get_cf_options<C: 'static + Column + ColumnName>(
    options: &BlockstoreOptions,
    oldest_slot: &OldestSlot,
) -> Options {
    let mut cf_options = Options::default();
    // 256 * 8 = 2GB. 6 of these columns should take at most 12GB of RAM
    cf_options.set_max_write_buffer_number(8);
    cf_options.set_write_buffer_size(MAX_WRITE_BUFFER_SIZE as usize);
    let file_num_compaction_trigger = 4;
    // Recommend that this be around the size of level 0. Level 0 estimated size in stable state is
    // write_buffer_size * min_write_buffer_number_to_merge * level0_file_num_compaction_trigger
    // Source: https://docs.rs/rocksdb/0.6.0/rocksdb/struct.Options.html#method.set_level_zero_file_num_compaction_trigger
    let total_size_base = MAX_WRITE_BUFFER_SIZE * file_num_compaction_trigger;
    let file_size_base = total_size_base / 10;
    cf_options.set_level_zero_file_num_compaction_trigger(file_num_compaction_trigger as i32);
    cf_options.set_max_bytes_for_level_base(total_size_base);
    cf_options.set_target_file_size_base(file_size_base);

    let disable_auto_compactions = should_disable_auto_compactions(&options.access_type);
    if disable_auto_compactions {
        cf_options.set_disable_auto_compactions(true);
    }

    if !disable_auto_compactions && should_enable_cf_compaction(C::NAME) {
        cf_options.set_compaction_filter_factory(PurgedSlotFilterFactory::<C> {
            oldest_slot: oldest_slot.clone(),
            name: CString::new(format!("purged_slot_filter_factory({})", C::NAME)).unwrap(),
            _phantom: PhantomData,
        });
    }

    process_cf_options_advanced::<C>(&mut cf_options, &options.column_options);

    cf_options
}

/// A CompactionFilter implementation to remove keys older than a given slot.
struct PurgedSlotFilter<C: Column + ColumnName> {
    /// The oldest slot to keep; any slot < oldest_slot will be removed
    oldest_slot: Slot,
    /// Whether to preserve keys that return slot 0, even when oldest_slot > 0.
    // This is used to delete old column data that wasn't keyed with a Slot, and so always returns
    // `C::slot() == 0`
    clean_slot_0: bool,
    name: CString,
    _phantom: PhantomData<C>,
}

impl<C: Column + ColumnName> CompactionFilter for PurgedSlotFilter<C> {
    fn filter(&mut self, _level: u32, key: &[u8], _value: &[u8]) -> CompactionDecision {
        use rocksdb::CompactionDecision::*;

        let slot_in_key = C::slot(C::index(key));
        if slot_in_key >= self.oldest_slot || (slot_in_key == 0 && !self.clean_slot_0) {
            Keep
        } else {
            Remove
        }
    }

    fn name(&self) -> &CStr {
        &self.name
    }
}

struct PurgedSlotFilterFactory<C: Column + ColumnName> {
    oldest_slot: OldestSlot,
    name: CString,
    _phantom: PhantomData<C>,
}

impl<C: Column + ColumnName> CompactionFilterFactory for PurgedSlotFilterFactory<C> {
    type Filter = PurgedSlotFilter<C>;

    fn create(&mut self, _context: CompactionFilterContext) -> Self::Filter {
        let copied_oldest_slot = self.oldest_slot.get();
        let copied_clean_slot_0 = self.oldest_slot.get_clean_slot_0();
        PurgedSlotFilter::<C> {
            oldest_slot: copied_oldest_slot,
            clean_slot_0: copied_clean_slot_0,
            name: CString::new(format!(
                "purged_slot_filter({}, {:?})",
                C::NAME,
                copied_oldest_slot
            ))
            .unwrap(),
            _phantom: PhantomData,
        }
    }

    fn name(&self) -> &CStr {
        &self.name
    }
}

fn process_cf_options_advanced<C: 'static + Column + ColumnName>(
    cf_options: &mut Options,
    column_options: &LedgerColumnOptions,
) {
    // Explicitly disable compression on all columns by default
    // See https://docs.rs/rocksdb/0.21.0/rocksdb/struct.Options.html#method.set_compression_type
    cf_options.set_compression_type(DBCompressionType::None);

    if should_enable_compression::<C>() {
        cf_options.set_compression_type(
            column_options
                .compression_type
                .to_rocksdb_compression_type(),
        );
    }
}

// Returns true if the column family enables compression.
fn should_enable_compression<C: 'static + Column + ColumnName>() -> bool {
    C::NAME == columns::TransactionStatus::NAME
}
