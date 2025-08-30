use rocksdb::perf::set_perf_stats;
use rocksdb::{PerfContext, PerfStatsLevel};
use solana_time_utils::timestamp;
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

// The minimum time duration between two RocksDB perf samples of the same operation.
const PERF_SAMPLING_MIN_DURATION: Duration = Duration::from_secs(1);

#[derive(Debug, Default)]
/// A struct that holds the current status of RocksDB perf sampling.
pub struct PerfSamplingStatus {
    // The number of RocksDB operations since the last perf sample.
    op_count: AtomicUsize,
    // The timestamp of the latest operation with perf stats collection.
    last_sample_time_ms: AtomicU64,
}

impl PerfSamplingStatus {
    fn should_sample(&self, sample_count_interval: usize) -> bool {
        if sample_count_interval == 0 {
            return false;
        }

        // Rate-limiting based on the number of samples.
        if self.op_count.fetch_add(1, Ordering::Relaxed) < sample_count_interval {
            return false;
        }
        self.op_count.store(0, Ordering::Relaxed);

        // Rate-limiting based on the time duration.
        let current_time_ms = timestamp();
        let old_time_ms = self.last_sample_time_ms.load(Ordering::Relaxed);
        if old_time_ms + (PERF_SAMPLING_MIN_DURATION.as_millis() as u64) > current_time_ms {
            return false;
        }

        // If the `last_sample_time_ms` has a different value than `old_time_ms`,
        // it means some other thread has performed the sampling and updated
        // the last sample time.  In this case, the current thread will skip
        // the current sample.
        self.last_sample_time_ms
            .compare_exchange_weak(
                old_time_ms,
                current_time_ms,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .is_ok()
    }
}

/// A metrics struct to track the number of times Blockstore RPC function are called.
#[derive(Default)]
pub(crate) struct BlockstoreRpcApiMetrics {
    pub num_get_block_height: AtomicU64,
    pub num_get_complete_transaction: AtomicU64,
    pub num_get_confirmed_signatures_for_address: AtomicU64,
    pub num_get_confirmed_signatures_for_address2: AtomicU64,
    pub num_get_rooted_block: AtomicU64,
    pub num_get_rooted_block_time: AtomicU64,
    pub num_get_rooted_transaction: AtomicU64,
    pub num_get_rooted_transaction_status: AtomicU64,
    pub num_get_rooted_block_with_entries: AtomicU64,
    pub num_get_transaction_status: AtomicU64,
}

// Thread local instance of RocksDB's PerfContext.
thread_local! {static PER_THREAD_ROCKS_PERF_CONTEXT: RefCell<PerfContext> = RefCell::new(PerfContext::default());}

/// The function enables RocksDB PerfContext once for every `sample_interval`.
///
/// PerfContext is a thread-local struct defined in RocksDB for collecting
/// per-thread read / write performance metrics.
///
/// When this function enables PerfContext, the function will return true,
/// and the PerfContext of the ubsequent RocksDB operation will be collected.
pub(crate) fn maybe_enable_rocksdb_perf(
    sample_interval: usize,
    perf_status: &PerfSamplingStatus,
) -> Option<Instant> {
    if perf_status.should_sample(sample_interval) {
        set_perf_stats(PerfStatsLevel::EnableTime);
        PER_THREAD_ROCKS_PERF_CONTEXT.with(|perf_context| {
            perf_context.borrow_mut().reset();
        });
        return Some(Instant::now());
    }
    None
}
