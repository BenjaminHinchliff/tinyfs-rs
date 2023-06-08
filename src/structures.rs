use std::{ffi::CString, mem, time::UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::{INode, Root, Stat, SuperBlock, TfsError, TfsResult, BLOCK_SIZE, DEFAULT_DISK_SIZE};

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
    #[allow(dead_code)]
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
// can't use struct size for Statdata due to padding
const INODE_BLOCKS: usize = (BLOCK_SIZE
    - mem::size_of::<[u8; MAX_FILENAME_LEN]>()
    - mem::size_of::<u16>()
    - mem::size_of::<u32>() * 3)
    / mem::size_of::<u16>();

#[derive(Debug, Serialize, Deserialize)]
pub struct StatData {
    pub size: u16,
    pub ctime: u32,
    pub mtime: u32,
    pub atime: u32,
}

impl StatData {
    pub fn new() -> Self {
        Self {
            size: 0,
            ctime: 0,
            mtime: 0,
            atime: 0,
        }
    }
}

impl From<Stat> for StatData {
    fn from(
        Stat {
            size,
            ctime,
            mtime,
            atime,
        }: Stat,
    ) -> Self {
        Self {
            size,
            ctime: ctime.duration_since(UNIX_EPOCH).unwrap().as_secs() as u32,
            mtime: mtime.duration_since(UNIX_EPOCH).unwrap().as_secs() as u32,
            atime: atime.duration_since(UNIX_EPOCH).unwrap().as_secs() as u32,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct INodeData {
    pub filename: [u8; MAX_FILENAME_LEN],
    pub stat: StatData,
    #[serde(with = "BigArray")]
    pub blocks: [u16; INODE_BLOCKS],
}

impl INodeData {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            filename: [0; MAX_FILENAME_LEN],
            stat: StatData::new(),
            blocks: [0; INODE_BLOCKS],
        }
    }
}

impl From<INode> for INodeData {
    fn from(
        INode {
            filename,
            stat,
            mut blocks,
            ..
        }: INode,
    ) -> Self {
        let filename = CString::new(filename).unwrap();
        let mut filename = filename.into_bytes();
        filename.resize(MAX_FILENAME_LEN, 0);
        blocks.resize(INODE_BLOCKS, 0);
        Self {
            filename: filename.try_into().unwrap(),
            stat: stat.into(),
            blocks: blocks.try_into().unwrap(),
        }
    }
}

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
