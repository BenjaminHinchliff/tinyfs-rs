use std::{ffi::CString, path::Path};

use disk::Disk;
use structures::{INodeData, ALLOCATION_TABLE_LEN};

use crate::structures::{RootData, SuperBlockData};

mod disk;
mod structures;

// hardcoded until const generics are stable
pub const BLOCK_SIZE: usize = 256;
pub const DEFAULT_DISK_SIZE: usize = 10240;

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
    #[error("Out of space")]
    OutOfSpace,
    #[error("File Referenced by file descriptor not found")]
    InvalidDesc,
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

    pub fn allocate_block(&mut self) -> Option<u16> {
        self.dirty = true;
        for (i, byte) in self.allocated_blocks.iter_mut().enumerate() {
            if *byte != u8::MAX {
                for bit in 0..8 {
                    if *byte & (1 << bit) == 0 {
                        *byte |= 1 << bit;
                        let block = i as u16 * 8 + bit;
                        return Some(block);
                    }
                }
            }
        }
        None
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
    size: u16,
    blocks: Vec<u16>,
}

impl INode {
    pub fn new(block: u16, filename: String) -> Self {
        Self {
            block,
            dirty: true,
            filename,
            size: 0,
            blocks: Vec::new(),
        }
    }

    pub fn from_block<const BLOCK_SIZE: usize>(
        block: u16,
        disk: &mut Disk<BLOCK_SIZE>,
    ) -> TfsResult<Self> {
        let data = disk.read_block(block as usize)?;
        let INodeData {
            filename,
            size,
            blocks,
        }: INodeData = bincode::deserialize(&data)?;

        let filename_len = filename.iter().position(|&b| b == 0);
        let filename = if let Some(filename_len) = filename_len {
            &filename[..filename_len]
        } else {
            &filename
        };

        Ok(Self {
            block,
            dirty: false,
            filename: CString::new(filename)?.into_string().unwrap(),
            size,
            blocks: blocks.iter().filter(|b| **b != 0).copied().collect(),
        })
    }

    pub fn push_block(&mut self, block: u16) {
        self.dirty = true;
        self.blocks.push(block);
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
        disk: &mut Disk<DISK_SIZE>,
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

    pub fn create_inode(&mut self, block: u16, filename: String) -> usize {
        self.dirty = true;
        self.inodes.push(INode::new(block, filename));
        self.inodes.len() - 1
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

#[derive(Debug, Clone, Copy)]
pub struct TfsDesc(usize);

#[derive(Debug, Clone, Copy)]
pub struct TfsFile {
    inode: usize,
    offset: usize,
}

#[derive(Debug)]
pub struct Tfs<const BLOCK_SIZE: usize> {
    superblock: SuperBlock,
    root: Root,
    open_files: Vec<TfsFile>,
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
            open_files: Vec::new(),
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
        let mut disk: Disk<BLOCK_SIZE> = Disk::open(path, 0)?;
        let superblock = disk.read_block(0)?;
        if superblock[0] != 0x5A {
            return Err(TfsError::MagicNumberError(superblock[0]));
        }
        let superblock: SuperBlockData = bincode::deserialize(&superblock)?;
        let root = disk.read_block(superblock.root_inode as usize)?;
        let root: RootData = bincode::deserialize(&root)?;
        Ok(Self {
            superblock: superblock.into(),
            root: Root::from_data(root, &mut disk)?,
            open_files: Vec::new(),
            disk,
        })
    }

    fn create_inode(&mut self, filename: String) -> TfsResult<usize> {
        let inode = self
            .superblock
            .allocate_block()
            .ok_or(TfsError::OutOfSpace)?;
        Ok(self.root.create_inode(inode, filename))
    }

    pub fn open(&mut self, filename: impl AsRef<Path>) -> TfsResult<TfsDesc> {
        let filename = filename.as_ref().to_str().unwrap();
        let inode = self
            .root
            .inodes
            .iter()
            .enumerate()
            .find(|(_, inode_)| inode_.filename == filename)
            .map(|(i, _)| i)
            .or_else(|| self.create_inode(filename.to_string()).ok());
        self.sync()?;
        if let Some(inode) = inode {
            self.open_files.push(TfsFile { inode, offset: 0 });
            Ok(TfsDesc(self.open_files.len() - 1))
        } else {
            Err(TfsError::OutOfSpace)
        }
    }

    pub fn write(&mut self, desc: TfsDesc, buf: &[u8]) -> TfsResult<()> {
        let file = self
            .open_files
            .get_mut(desc.0)
            .ok_or(TfsError::InvalidDesc)?;
        let inode = self.root.inodes.get_mut(file.inode).unwrap();
        for bytes in buf.chunks(BLOCK_SIZE) {
            let block = self
                .superblock
                .allocate_block()
                .ok_or(TfsError::OutOfSpace)?;
            inode.push_block(block);
            let bytes_written = bytes.len();
            let bytes = if bytes.len() == BLOCK_SIZE {
                bytes.try_into().unwrap()
            } else {
                let mut bytes = bytes.to_vec();
                bytes.resize(BLOCK_SIZE, 0);
                bytes.try_into().unwrap()
            };
            self.disk.write_block(block as usize, bytes)?;
            inode.size += bytes_written as u16;
            file.offset += bytes_written;
        }
        file.offset = 0;
        self.sync()?;
        Ok(())
    }

    pub fn read_byte(&mut self, desc: TfsDesc) -> TfsResult<Option<u8>> {
        let file = self
            .open_files
            .get_mut(desc.0)
            .ok_or(TfsError::InvalidDesc)?;
        let inode = self.root.inodes.get_mut(file.inode).unwrap();
        if file.offset >= inode.size as usize {
            return Ok(None);
        }
        let block = inode.blocks.get(file.offset / BLOCK_SIZE).unwrap();
        let block = self.disk.read_block(*block as usize)?;
        let byte = block[file.offset % BLOCK_SIZE];
        file.offset += 1;
        Ok(Some(byte))
    }

    pub fn sync(&mut self) -> TfsResult<()> {
        self.superblock.sync(&mut self.disk)?;
        self.root.sync(&mut self.disk)?;
        Ok(())
    }
}

impl<const BLOCK_SIZE: usize> Drop for Tfs<BLOCK_SIZE> {
    fn drop(&mut self) {
        self.sync().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn mkfs_works() {
        const DISK_PATH: &str = "mkfs-disk.bin";
        Tfs::<BLOCK_SIZE>::mkfs(DISK_PATH, DEFAULT_DISK_SIZE).unwrap();
        let mut disk: Disk<BLOCK_SIZE> = Disk::open(DISK_PATH, DEFAULT_DISK_SIZE).unwrap();
        let superblock = disk.read_block(0).unwrap();
        let superblock: SuperBlockData = bincode::deserialize(&superblock).unwrap();
        assert_eq!(superblock.magic_number, 0x5A);
        assert_eq!(superblock.root_inode, 1);
        fs::remove_file(DISK_PATH).unwrap();
    }

    #[test]
    fn mount_works() {
        const DISK_PATH: &str = "mount-disk.bin";
        Tfs::<BLOCK_SIZE>::mkfs(DISK_PATH, DEFAULT_DISK_SIZE).unwrap();
        let _tfs = Tfs::<BLOCK_SIZE>::mount(DISK_PATH).unwrap();
        fs::remove_file(DISK_PATH).unwrap();
    }

    #[test]
    fn open_works() {
        const DISK_PATH: &str = "open-disk.bin";
        Tfs::<BLOCK_SIZE>::mkfs(DISK_PATH, DEFAULT_DISK_SIZE).unwrap();
        let mut tfs = Tfs::<BLOCK_SIZE>::mount(DISK_PATH).unwrap();
        let _desc = tfs.open("test.txt").unwrap();
        fs::remove_file(DISK_PATH).unwrap();
    }

    #[test]
    fn write_works() {
        const DISK_PATH: &str = "write-disk.bin";
        Tfs::<BLOCK_SIZE>::mkfs(DISK_PATH, DEFAULT_DISK_SIZE).unwrap();
        {
            let mut tfs = Tfs::<BLOCK_SIZE>::mount(DISK_PATH).unwrap();
            let desc = tfs.open("test.txt").unwrap();
            tfs.write(desc, &"Hello, World!".as_bytes()).unwrap();
            let harry = include_bytes!("../harry-sm.jpg");
            let desc2 = tfs.open("cat.jpg").unwrap();
            tfs.write(desc2, harry).unwrap();
        }
        {
            let tfs = Tfs::<BLOCK_SIZE>::mount(DISK_PATH).unwrap();
            assert_eq!(tfs.root.inodes.len(), 2);
        }
        fs::remove_file(DISK_PATH).unwrap();
    }
}
