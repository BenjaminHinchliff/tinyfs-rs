use std::mem;

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::{TfsError, TfsResult, BLOCK_SIZE, DEFAULT_DISK_SIZE};

const ALLOCATION_TABLE_LEN: usize = BLOCK_SIZE - mem::size_of::<u8>() - mem::size_of::<u16>();
const MAX_BLOCKS: usize = (ALLOCATION_TABLE_LEN) * 8;

#[derive(Debug, Serialize, Deserialize)]
pub struct SuperBlock {
    magic_number: u8,
    root_inode: u16,
    #[serde(with = "BigArray")]
    allocated_blocks: [u8; ALLOCATION_TABLE_LEN],
}

impl SuperBlock {
    pub fn new(block_size: usize) -> TfsResult<Self> {
        Self::new_with_size(block_size, DEFAULT_DISK_SIZE)
    }

    pub fn new_with_size(block_size: usize, size: usize) -> TfsResult<Self> {
        // subtract size of magic number and root inode
        let blocks = size / block_size;
        let allocated_needed = blocks / 8 + if blocks % 8 != 0 { 1 } else { 0 };
        if allocated_needed > MAX_BLOCKS {
            return Err(TfsError::SizeError { size });
        }
        Ok(Self {
            magic_number: 0x5A,
            root_inode: 0,
            allocated_blocks: [0; ALLOCATION_TABLE_LEN],
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct INode {
    #[serde(with = "BigArray")]
    blocks: [u16; BLOCK_SIZE / mem::size_of::<u16>()],
}

impl INode {
    pub fn new() -> Self {
        Self {
            blocks: [0; BLOCK_SIZE / mem::size_of::<u16>()],
        }
    }
}

pub type Data = [u8; BLOCK_SIZE];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn superblock_correct_size() {
        let super_block = SuperBlock::new(BLOCK_SIZE).unwrap();
        let encoded = bincode::serialize(&super_block).unwrap();
        assert_eq!(encoded.len(), BLOCK_SIZE);
    }

    #[test]
    fn inode_correct_size() {
        let inode = INode::new();
        let encoded = bincode::serialize(&inode).unwrap();
        assert_eq!(encoded.len(), BLOCK_SIZE);
    }
}
