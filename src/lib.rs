use std::{
    cell::RefCell,
    ffi::CString,
    path::Path,
    time::{Duration, SystemTime},
};

use disk::Disk;
use structures::{INodeData, StatData, ALLOCATION_TABLE_LEN};

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
    #[error("Unable to find file {0}")]
    FileNotFound(String),
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
pub struct Stat {
    pub size: u16,
    pub ctime: SystemTime,
    pub mtime: SystemTime,
    pub atime: SystemTime,
}

impl Stat {
    pub fn new() -> Self {
        Self {
            size: 0,
            ctime: SystemTime::now(),
            mtime: SystemTime::now(),
            atime: SystemTime::now(),
        }
    }
}

impl From<StatData> for Stat {
    fn from(
        StatData {
            size,
            ctime,
            mtime,
            atime,
        }: StatData,
    ) -> Self {
        Self {
            size,
            ctime: SystemTime::UNIX_EPOCH + Duration::from_secs(ctime as u64),
            mtime: SystemTime::UNIX_EPOCH + Duration::from_secs(mtime as u64),
            atime: SystemTime::UNIX_EPOCH + Duration::from_secs(atime as u64),
        }
    }
}

#[derive(Debug, Clone)]
struct INode {
    block: u16,
    dirty: bool,
    filename: String,
    stat: Stat,
    blocks: Vec<u16>,
}

impl INode {
    pub fn new(block: u16, filename: String) -> Self {
        Self {
            block,
            dirty: true,
            filename,
            stat: Stat::new(),
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
            stat,
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
            stat: stat.into(),
            blocks: blocks.iter().filter(|b| **b != 0).copied().collect(),
        })
    }

    pub fn push_block(&mut self, block: u16) {
        self.dirty = true;
        self.blocks.push(block);
    }

    pub fn sync(&mut self, disk: &mut Disk<BLOCK_SIZE>) -> TfsResult<()> {
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

    pub fn sync(&mut self, disk: &mut Disk<BLOCK_SIZE>) -> TfsResult<()> {
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

#[derive(Debug, Clone)]
pub struct ReadDirEntry {
    pub filename: String,
    pub stat: Stat,
}

#[derive(Debug)]
pub struct TfsFsFile {
    inode: usize,
    offset: usize,
}

#[derive(Debug)]
pub struct TfsFile<'a> {
    filesystem: &'a RefCell<TfsFs>,
    file: TfsFsFile,
}

impl<'a> TfsFile<'a> {
    pub fn write(&mut self, buf: &[u8]) -> TfsResult<()> {
        self.filesystem.borrow_mut().write(&mut self.file, buf)
    }

    pub fn read_byte(&mut self) -> TfsResult<Option<u8>> {
        self.filesystem.borrow_mut().read_byte(&mut self.file)
    }

    pub fn rename(&mut self, newname: &str) -> TfsResult<()> {
        self.filesystem.borrow_mut().rename(&mut self.file, newname)
    }

    pub fn stat(&self, file: TfsFsFile) -> TfsResult<Stat> {
        self.filesystem.borrow_mut().stat(file)
    }
}

#[derive(Debug)]
pub struct Tfs {
    tfs: RefCell<TfsFs>,
}

impl Tfs {
    pub fn new(disk: Disk<BLOCK_SIZE>) -> Self {
        Self {
            tfs: RefCell::new(TfsFs::new(disk)),
        }
    }

    pub fn mkfs(path: impl AsRef<Path>, size: usize) -> TfsResult<()> {
        TfsFs::mkfs(path, size)
    }

    pub fn mount(path: impl AsRef<Path>) -> TfsResult<Self> {
        let tfs = TfsFs::mount(path)?;
        Ok(Self {
            tfs: RefCell::new(tfs),
        })
    }

    pub fn readdir<'a>(&'a self) -> Vec<ReadDirEntry> {
        self.tfs.borrow().readdir().collect()
    }

    pub fn open(&mut self, filename: impl AsRef<Path>) -> TfsResult<TfsFile> {
        let mut tfs = self.tfs.borrow_mut();
        let file = tfs.open(filename)?;
        Ok(TfsFile {
            filesystem: &self.tfs,
            file,
        })
    }

    pub fn sync(&mut self) -> TfsResult<()> {
        // TODO: sync only this file not the whole filesystem
        self.tfs.borrow_mut().sync()
    }
}

impl Drop for Tfs {
    fn drop(&mut self) {
        self.sync().unwrap();
    }
}

#[derive(Debug)]
pub struct TfsFs {
    superblock: SuperBlock,
    root: Root,
    disk: Disk<BLOCK_SIZE>,
}

impl TfsFs {
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
        TfsFs::new(disk).sync()?;

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

    pub fn open(&mut self, filename: impl AsRef<Path>) -> TfsResult<TfsFsFile> {
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
            self.root.inodes[inode].stat.atime = SystemTime::now();
            Ok(TfsFsFile { inode, offset: 0 })
        } else {
            Err(TfsError::OutOfSpace)
        }
    }

    pub fn close(&mut self, _file: &mut TfsFsFile) -> TfsResult<()> {
        self.sync()
    }

    pub fn write(&mut self, file: &mut TfsFsFile, buf: &[u8]) -> TfsResult<()> {
        let inode = self.root.inodes.get_mut(file.inode).unwrap();
        inode.stat.mtime = SystemTime::now();
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
            inode.stat.size += bytes_written as u16;
            file.offset += bytes_written;
        }
        file.offset = 0;
        self.sync()?;
        Ok(())
    }

    pub fn read_byte(&mut self, file: &mut TfsFsFile) -> TfsResult<Option<u8>> {
        let inode = self.root.inodes.get_mut(file.inode).unwrap();
        inode.stat.atime = SystemTime::now();
        if file.offset >= inode.stat.size as usize {
            return Ok(None);
        }
        let block = inode.blocks.get(file.offset / BLOCK_SIZE).unwrap();
        let block = self.disk.read_block(*block as usize)?;
        let byte = block[file.offset % BLOCK_SIZE];
        file.offset += 1;
        Ok(Some(byte))
    }

    pub fn readdir<'a>(&'a self) -> impl Iterator<Item = ReadDirEntry> + 'a {
        self.root
            .inodes
            .iter()
            .map(|INode { filename, stat, .. }| ReadDirEntry {
                filename: filename.to_string(),
                stat: stat.clone(),
            })
    }

    pub fn rename(&mut self, file: &mut TfsFsFile, newname: &str) -> TfsResult<()> {
        let inode = self.root.inodes.get_mut(file.inode).unwrap();
        inode.stat.mtime = SystemTime::now();
        inode.filename = newname.to_string();
        Ok(())
    }

    pub fn stat(&self, file: TfsFsFile) -> TfsResult<Stat> {
        let inode = self.root.inodes.get(file.inode).unwrap();
        Ok(inode.stat.clone())
    }

    pub fn sync(&mut self) -> TfsResult<()> {
        self.superblock.sync(&mut self.disk)?;
        self.root.sync(&mut self.disk)?;
        Ok(())
    }
}

impl Drop for TfsFs {
    fn drop(&mut self) {
        // nothing can be done if sync fails in drop
        self.sync().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn mkfs_works() {
        const DISK_PATH: &str = "mkfs-disk.bin";
        TfsFs::mkfs(DISK_PATH, DEFAULT_DISK_SIZE).unwrap();
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
        TfsFs::mkfs(DISK_PATH, DEFAULT_DISK_SIZE).unwrap();
        let _tfs = TfsFs::mount(DISK_PATH).unwrap();
        fs::remove_file(DISK_PATH).unwrap();
    }

    #[test]
    fn open_works() {
        const DISK_PATH: &str = "open-disk.bin";
        TfsFs::mkfs(DISK_PATH, DEFAULT_DISK_SIZE).unwrap();
        let mut tfs = TfsFs::mount(DISK_PATH).unwrap();
        let _desc = tfs.open("test.txt").unwrap();
        fs::remove_file(DISK_PATH).unwrap();
    }

    #[test]
    fn write_works() {
        const DISK_PATH: &str = "write-disk.bin";
        TfsFs::mkfs(DISK_PATH, DEFAULT_DISK_SIZE).unwrap();
        {
            let mut tfs = TfsFs::mount(DISK_PATH).unwrap();
            let mut desc = tfs.open("test.txt").unwrap();
            tfs.write(&mut desc, &"Hello, World!".as_bytes()).unwrap();
            let harry = include_bytes!("../harry-sm.jpg");
            let mut desc2 = tfs.open("cat.jpg").unwrap();
            tfs.write(&mut desc2, harry).unwrap();
        }
        {
            let tfs = TfsFs::mount(DISK_PATH).unwrap();
            assert_eq!(tfs.root.inodes.len(), 2);
        }
        fs::remove_file(DISK_PATH).unwrap();
    }
}
