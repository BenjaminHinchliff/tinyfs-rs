mod disk;
mod structures;

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use std::mem;

// hardcoded until const generics are stable
const BLOCK_SIZE: usize = 256;
const DEFAULT_DISK_SIZE: usize = 10240;

#[derive(Debug, thiserror::Error)]
pub enum TfsError {
    #[error("Disk size of {size} too large to fit in superblock")]
    SizeError { size: usize },
    #[error("Disk IO Error: {0}")]
    DiskError(#[from] disk::DiskError),
}

pub type TfsResult<T> = Result<T, TfsError>;
