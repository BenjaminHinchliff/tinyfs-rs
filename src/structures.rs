use std::{ffi::CString, mem};

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::{INode, Root, SuperBlock, TfsError, TfsResult, BLOCK_SIZE, DEFAULT_DISK_SIZE};

pub const ALLOCATION_TABLE_LEN: usize = BLOCK_SIZE - mem::size_of::<u8>() - mem::size_of::<u16>();
const MAX_BLOCKS: usize = (ALLOCATION_TABLE_LEN) * 8;

#[derive(Debug, Serialize, Deserialize)]
pub struct SuperBlockData {
    pub magic_number: u8,
    pub root_inode: u16,
    #[serde(with = "BigArray")]
    pub allocated_blocks: [u8; ALLOCATION_TABLE_LEN],
}

impl SuperBlockData {
    pub fn new(root_inode: u16) -> TfsResult<Self> {
        Self::new_with_size(root_inode, DEFAULT_DISK_SIZE)
    }

    pub fn new_with_size(root_inode: u16, size: usize) -> TfsResult<Self> {
        // subtract size of magic number and root inode
        let blocks = size / BLOCK_SIZE;
        let allocated_needed = blocks / 8 + if blocks % 8 != 0 { 1 } else { 0 };
        if allocated_needed > MAX_BLOCKS {
            return Err(TfsError::SizeError { size });
        }
        Ok(Self {
            magic_number: 0x5A,
            root_inode,
            allocated_blocks: [0; ALLOCATION_TABLE_LEN],
        })
    }
}

impl From<SuperBlock> for SuperBlockData {
    fn from(
        SuperBlock {
            allocated_blocks, ..
        }: SuperBlock,
    ) -> Self {
        Self {
            magic_number: 0x5A,
            root_inode: 1,
            allocated_blocks,
        }
    }
}

const ROOT_INODES: usize = BLOCK_SIZE / mem::size_of::<u16>();

#[derive(Debug, Serialize, Deserialize)]
pub struct RootData {
    #[serde(with = "BigArray")]
    pub inodes: [u16; ROOT_INODES],
}

impl RootData {
    pub fn new() -> Self {
        Self {
            inodes: [0; ROOT_INODES],
        }
    }
}

impl TryFrom<Root> for RootData {
    type Error = TfsError;

    fn try_from(Root { inodes, .. }: Root) -> Result<Self, Self::Error> {
        let mut inodes: Vec<u16> = inodes.into_iter().map(|inode| inode.block).collect();
        if inodes.len() > ROOT_INODES {
            return Err(TfsError::SizeError { size: inodes.len() });
        }
        inodes.resize(ROOT_INODES, 0);
        Ok(Self {
            inodes: inodes.try_into().unwrap(),
        })
    }
}

const MAX_FILENAME_LEN: usize = 8;
const INODE_BLOCKS: usize =
    (BLOCK_SIZE - mem::size_of::<[u8; MAX_FILENAME_LEN]>()) / mem::size_of::<u16>();

#[derive(Debug, Serialize, Deserialize)]
pub struct INodeData {
    pub filename: [u8; MAX_FILENAME_LEN],
    #[serde(with = "BigArray")]
    pub blocks: [u16; INODE_BLOCKS],
}

impl INodeData {
    pub fn new() -> Self {
        Self {
            filename: [0; MAX_FILENAME_LEN],
            blocks: [0; INODE_BLOCKS],
        }
    }
}

impl From<INode> for INodeData {
    fn from(
        INode {
            filename,
            mut data_blocks,
            ..
        }: INode,
    ) -> Self {
        let filename = CString::new(filename).unwrap();
        let mut filename = filename.into_bytes();
        filename.resize(MAX_FILENAME_LEN, 0);
        data_blocks.resize(INODE_BLOCKS, 0);
        Self {
            filename: filename.try_into().unwrap(),
            blocks: data_blocks.try_into().unwrap(),
        }
    }
}

pub type Data = [u8; BLOCK_SIZE];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn superblock_correct_size() {
        let super_block = SuperBlockData::new(1).unwrap();
        let encoded = bincode::serialize(&super_block).unwrap();
        assert_eq!(encoded.len(), BLOCK_SIZE);
    }

    #[test]
    fn inode_correct_size() {
        let inode = INodeData::new();
        let encoded = bincode::serialize(&inode).unwrap();
        assert_eq!(encoded.len(), BLOCK_SIZE);
    }

    #[test]
    fn root_correct_size() {
        let inode = RootData::new();
        let encoded = bincode::serialize(&inode).unwrap();
        assert_eq!(encoded.len(), BLOCK_SIZE);
    }
}
