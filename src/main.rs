use std::{io::Write, println};

use anyhow::Result;
use tempfile::NamedTempFile;
use tinyfs_rs::{Tfs, BLOCK_SIZE, DEFAULT_DISK_SIZE};

fn main() -> Result<()> {
    {
        println!("making filesystem...");
        Tfs::<BLOCK_SIZE>::mkfs("test.bin", DEFAULT_DISK_SIZE)?;
        println!("mouting filesystem...");
        let mut tfs = Tfs::<BLOCK_SIZE>::mount("test.bin")?;
        println!("creating test.txt - a file containing \"Hello, World!\"");
        let desc = tfs.open("test.txt")?;
        tfs.write(desc, &"Hello, World!".as_bytes())?;
        println!("creating cat.jpg - a file containing a picture of a cat");
        let harry = include_bytes!("../harry-sm.jpg");
        let desc2 = tfs.open("cat.jpg")?;
        tfs.write(desc2, harry)?;
        println!("unmounting filesystem...");
    }
    {
        println!("mouting filesystem...");
        let mut tfs = Tfs::<BLOCK_SIZE>::mount("test.bin")?;

        println!("reading test.txt");
        let desc = tfs.open("test.txt")?;
        let mut hello = String::new();
        while let Some(byte) = tfs.read_byte(desc)? {
            hello.push(byte as char);
        }
        println!("contents: \"{}\"", hello);

        println!("reading cat.jpg");
        let desc2 = tfs.open("cat.jpg")?;
        let mut cat = Vec::new();
        while let Some(byte) = tfs.read_byte(desc2)? {
            cat.push(byte);
        }

        println!("opening cat.jpg in default image viewer...");
        let mut file = NamedTempFile::new()?;
        file.write_all(&cat)?;
        open::that(file.path())?;
    }
    Ok(())
}
