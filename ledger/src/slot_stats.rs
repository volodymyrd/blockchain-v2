use bitflags::bitflags;
use lru::LruCache;
use solana_clock::Slot;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Mutex;

bitflags! {
    #[derive(Copy, Clone, Default)]
    struct SlotFlags: u8 {
        const DEAD   = 0b00000001;
        const FULL   = 0b00000010;
        const ROOTED = 0b00000100;
    }
}

#[derive(Clone, Default)]
pub struct SlotStats {
    turbine_fec_set_index_counts: HashMap</*fec_set_index*/ u32, /*count*/ usize>,
    num_repaired: usize,
    num_recovered: usize,
    last_index: u64,
    flags: SlotFlags,
}

pub struct SlotsStats {
    pub stats: Mutex<LruCache<Slot, SlotStats>>,
}

const SLOTS_STATS_CACHE_CAPACITY: NonZeroUsize = NonZeroUsize::new(300).unwrap();

impl Default for SlotsStats {
    fn default() -> Self {
        Self {
            stats: Mutex::new(LruCache::new(SLOTS_STATS_CACHE_CAPACITY)),
        }
    }
}
