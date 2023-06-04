use std::{ffi::CString, path::Path};

use disk::Disk;
use structures::{INodeData, ALLOCATION_TABLE_LEN};

use crate::structures::{RootData, SuperBlockData};

mod disk;
mod structures;

// hardcoded until const generics are stable
const BLOCK_SIZE: usize = 256;
const DEFAULT_DISK_SIZE: usize = 10240;

#[derive(Debug, thiserror::Error)]
pub enum TfsError {
    #[error("Disk size of {size} too large to fit in superblock")]
    SizeError { size: usize },
    #[error("Disk IO Error: {0}")]
    DiskError(#[from] disk::DiskError),
    #[error("Serialization Error: {0}")]
    SerializationError(#[from] bincode::Error),
    #[error("Invalid magic number: {0} should be 0x5A")]
    MagicNumberError(u8),
    #[error("Invalid filename: {0}")]
    FilenameError(#[from] std::ffi::NulError),
}

pub type TfsResult<T> = Result<T, TfsError>;

#[derive(Debug, Clone)]
pub struct SuperBlock {
    dirty: bool,
    allocated_blocks: [u8; ALLOCATION_TABLE_LEN],
}

impl SuperBlock {
    pub fn new() -> Self {
        Self {
            dirty: true,
            allocated_blocks: [0; ALLOCATION_TABLE_LEN],
        }
    }

    pub fn mark_allocated(&mut self, block: u16) {
        self.dirty = true;
        let byte = block / 8;
        let bit = block % 8;
        self.allocated_blocks[byte as usize] |= 1 << bit;
    }

    pub fn mark_free(&mut self, block: u16) {
        self.dirty = true;
        let byte = block / 8;
        let bit = block % 8;
        self.allocated_blocks[byte as usize] &= !(1 << bit);
    }

    pub fn sync<const BLOCK_SIZE: usize>(&mut self, disk: &mut Disk<BLOCK_SIZE>) -> TfsResult<()> {
        if self.dirty {
            disk.write_block(
                0,
                bincode::serialize(&SuperBlockData::from(self.clone()))?
                    .try_into()
                    .unwrap(),
            )?;
            self.dirty = false;
        }
        Ok(())
    }
}

impl From<SuperBlockData> for SuperBlock {
    fn from(
        SuperBlockData {
            allocated_blocks, ..
        }: SuperBlockData,
    ) -> Self {
        Self {
            dirty: false,
            allocated_blocks,
        }
    }
}

#[derive(Debug, Clone)]
struct INode {
    block: u16,
    dirty: bool,
    filename: String,
    data_blocks: Vec<u16>,
}

impl INode {
    pub fn from_block<const BLOCK_SIZE: usize>(
        block: u16,
        disk: &Disk<BLOCK_SIZE>,
    ) -> TfsResult<Self> {
        let data = disk.read_block(block as usize)?;
        let INodeData { filename, blocks }: INodeData = bincode::deserialize(&data)?;

        Ok(Self {
            block,
            dirty: false,
            filename: CString::new(filename)?.into_string().unwrap(),
            data_blocks: blocks.iter().filter(|b| **b != 0).copied().collect(),
        })
    }

    pub fn sync<const BLOCK_SIZE: usize>(&mut self, disk: &mut Disk<BLOCK_SIZE>) -> TfsResult<()> {
        if self.dirty {
            disk.write_block(
                self.block as usize,
                bincode::serialize(&INodeData::from(self.clone()))?
                    .try_into()
                    .unwrap(),
            )?;
            self.dirty = false;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Root {
    dirty: bool,
    inodes: Vec<INode>,
}

impl Root {
    pub fn new() -> Self {
        Self {
            dirty: true,
            inodes: Vec::new(),
        }
    }

    pub fn from_data<const DISK_SIZE: usize>(
        data: RootData,
        disk: &Disk<DISK_SIZE>,
    ) -> TfsResult<Self> {
        let mut inodes = Vec::new();
        for block in data.inodes.into_iter().filter(|b| *b != 0) {
            inodes.push(INode::from_block(block, disk)?);
        }
        Ok(Self {
            dirty: false,
            inodes,
        })
    }

    pub fn sync<const BLOCK_SIZE: usize>(&mut self, disk: &mut Disk<BLOCK_SIZE>) -> TfsResult<()> {
        for inode in self.inodes.iter_mut() {
            inode.sync(disk)?;
        }
        if self.dirty {
            disk.write_block(
                1,
                bincode::serialize(&RootData::try_from(self.clone())?)?
                    .try_into()
                    .unwrap(),
            )?;
            self.dirty = false;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Tfs<const BLOCK_SIZE: usize> {
    superblock: SuperBlock,
    root: Root,
    disk: Disk<BLOCK_SIZE>,
}

impl<const BLOCK_SIZE: usize> Tfs<BLOCK_SIZE> {
    pub fn new(disk: Disk<BLOCK_SIZE>) -> Self {
        let mut superblock = SuperBlock::new();
        superblock.mark_allocated(0);
        superblock.mark_allocated(1);
        Self {
            superblock,
            root: Root::new(),
            disk,
        }
    }

    pub fn mkfs(path: impl AsRef<Path>, size: usize) -> TfsResult<()> {
        let mut disk: Disk<BLOCK_SIZE> = Disk::open(path, size)?;
        for i in 0..(size / BLOCK_SIZE) {
            disk.write_block(i, [0; BLOCK_SIZE])?;
        }
        Tfs::new(disk).sync()?;

        Ok(())
    }

    pub fn mount(path: impl AsRef<Path>) -> TfsResult<Self> {
        let disk: Disk<BLOCK_SIZE> = Disk::open(path, 0)?;
        let superblock = disk.read_block(0)?;
        if superblock[0] != 0x5A {
            return Err(TfsError::MagicNumberError(superblock[0]));
        }
        let superblock: SuperBlockData = bincode::deserialize(&superblock)?;
        let root = disk.read_block(superblock.root_inode as usize)?;
        let root: RootData = bincode::deserialize(&root)?;
        Ok(Self {
            superblock: superblock.into(),
            root: Root::from_data(root, &disk)?,
            disk,
        })
    }

    pub fn sync(&mut self) -> TfsResult<()> {
        self.superblock.sync(&mut self.disk)?;
        self.root.sync(&mut self.disk)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn mkfs_works() {
        Tfs::<BLOCK_SIZE>::mkfs("mkfs-disk.bin", DEFAULT_DISK_SIZE).unwrap();
        let disk: Disk<BLOCK_SIZE> = Disk::open("mkfs-disk.bin", DEFAULT_DISK_SIZE).unwrap();
        let superblock = disk.read_block(0).unwrap();
        let superblock: SuperBlockData = bincode::deserialize(&superblock).unwrap();
        assert_eq!(superblock.magic_number, 0x5A);
        assert_eq!(superblock.root_inode, 1);
        fs::remove_file("mkfs-disk.bin").unwrap();
    }

    #[test]
    fn mount_works() {
        Tfs::<BLOCK_SIZE>::mkfs("mount-disk.bin", DEFAULT_DISK_SIZE).unwrap();
        let _tfs = Tfs::<BLOCK_SIZE>::mount("mount-disk.bin").unwrap();
        fs::remove_file("mount-disk.bin").unwrap();
    }
}
