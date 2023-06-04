use std::{
    fs::{File, OpenOptions},
    os::unix::prelude::FileExt,
    path::Path,
};

#[derive(Debug, thiserror::Error)]
pub enum DiskError {
    #[error("Disk IO Error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Disk Size is Invalid - disk size must be a multiple of {block_size}")]
    InvalidSize { block_size: usize },
}

pub type DiskResult<T> = Result<T, DiskError>;

#[derive(Debug)]
pub struct Disk<const BLOCK_SIZE: usize> {
    backing_file: File,
}

impl<const BLOCK_SIZE: usize> Disk<BLOCK_SIZE> {
    pub fn open(path: impl AsRef<Path>, size: usize) -> DiskResult<Disk<BLOCK_SIZE>> {
        if size % BLOCK_SIZE != 0 {
            return Err(DiskError::InvalidSize {
                block_size: BLOCK_SIZE,
            });
        }

        let backing_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;
        Ok(Disk { backing_file })
    }

    pub fn read_block(&self, num: usize) -> DiskResult<[u8; BLOCK_SIZE]> {
        let mut block = [0; BLOCK_SIZE];
        self.backing_file
            .read_exact_at(&mut block, (num * BLOCK_SIZE) as u64)?;
        Ok(block)
    }

    pub fn write_block(&mut self, num: usize, data: [u8; BLOCK_SIZE]) -> DiskResult<()> {
        self.backing_file
            .write_all_at(&data, (num * BLOCK_SIZE) as u64)?;
        Ok(())
    }

    // rust doesn't need to have you explicitly close a file, instead linking it to the lifetime of
    // the `File` object, as such we don't need to implement close for this struct
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn write_read_works() {
        const BLOCK_SIZE: usize = 512;
        let mut disk: Disk<BLOCK_SIZE> = Disk::open("disk.bin", BLOCK_SIZE * 32).unwrap();
        let block = [0x42; BLOCK_SIZE];
        disk.write_block(15, block).unwrap();
        assert_eq!(disk.read_block(15).unwrap(), block);
        fs::remove_file("disk.bin").unwrap();
    }
}
