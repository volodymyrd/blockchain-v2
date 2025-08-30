use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};

pub const MAX_DATA_SHREDS_PER_SLOT: usize = 32_768;

#[repr(u8)]
#[derive(
    Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize, IntoPrimitive, Serialize, TryFromPrimitive,
)]
#[serde(into = "u8", try_from = "u8")]
pub enum ShredType {
    Data = 0b1010_0101,
    Code = 0b0101_1010,
}
