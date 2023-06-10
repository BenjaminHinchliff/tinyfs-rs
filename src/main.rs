use std::{thread, time::Duration};

use ansi_colours::ColourExt;
use ansi_term::Color;
use anyhow::Result;
use chrono::{DateTime, Local};
use image::{imageops, Pixel, Rgb, RgbImage};
use supports_color::{ColorLevel, Stream};
use tinyfs_rs::{Tfs, DEFAULT_DISK_SIZE};

fn to_ascii(image: &RgbImage, color_support: Option<ColorLevel>) -> String {
    let (width, height) = image.dimensions();
    let mut ascii = String::new();

    let mut last_color: Option<Color> = None;
    for y in 0..height {
        for x in 0..width {
            let pixel = image.get_pixel(x, y);
            let intensity = pixel.to_luma()[0];
            let ascii_char = intensity_to_ascii(intensity);
            if let Some(color_level) = color_support {
                let color = if color_level.has_16m {
                    rgb_to_color(pixel)
                } else if color_level.has_256 {
                    rgb_to_color(pixel).to_256()
                } else {
                    Color::White
                };
                let ansi = if let Some(last_color) = last_color {
                    last_color.infix(color).to_string()
                } else {
                    color.prefix().to_string()
                };
                ascii.push_str(&ansi);
                last_color = Some(color);
            }
            ascii.push(ascii_char);
        }
        ascii.push('\n');
    }
    if let Some(color) = last_color {
        ascii.push_str(&color.suffix().to_string())
    }

    ascii
}

fn rgb_to_color(rgb: &Rgb<u8>) -> Color {
    Color::RGB(rgb[0], rgb[1], rgb[2])
}

fn intensity_to_ascii(intensity: u8) -> char {
    let ascii_chars = [' ', '░', '▒', '▓', '█'];
    let num_chars = ascii_chars.len();

    let scaled_intensity = (intensity as usize * (num_chars - 1)) / u8::MAX as usize;
    ascii_chars[scaled_intensity]
}

fn ls(tfs: &Tfs) -> Result<()> {
    println!("listing files...");
    for f in tfs.readdir() {
        println!(
            " - {} created: {} modified: {} accessed: {}",
            f.filename,
            DateTime::<Local>::from(f.stat.ctime).format("%H:%M:%S"),
            DateTime::<Local>::from(f.stat.mtime).format("%H:%M:%S"),
            DateTime::<Local>::from(f.stat.atime).format("%H:%M:%S"),
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    const DISK_PATH: &str = "demo.disk";
    {
        println!("making filesystem...");
        Tfs::mkfs(DISK_PATH, DEFAULT_DISK_SIZE)?;
        println!("mouting filesystem...");
        let mut tfs = Tfs::mount(DISK_PATH)?;
        println!("creating test.txt - a file containing \"Hello, World!\"");
        let mut file1 = tfs.open("test.txt")?;
        file1.write(b"Hello, World!")?;
        println!("creating cat.jpg - a file containing a picture of a cat");
        let harry = include_bytes!("../harry-sm.jpg");
        let mut file2 = tfs.open("cat.jpg")?;
        file2.write(harry)?;
        println!("unmounting filesystem...");
    }
    println!("sleeping so timestamps can change...");
    thread::sleep(Duration::from_secs_f32(1.5));
    {
        println!("mouting filesystem...");
        let mut tfs = Tfs::mount(DISK_PATH)?;

        ls(&tfs)?;

        println!("rename cat.jpg");
        let mut hary = tfs.open("cat.jpg")?;
        hary.rename("hary.jpg")?;

        ls(&tfs)?;

        println!("reading test.txt");
        let mut test = tfs.open("test.txt")?;
        let mut hello = String::new();
        while let Some(byte) = test.read_byte()? {
            hello.push(byte as char);
        }
        println!("contents: \"{}\"", hello);

        println!("reading hary.jpg");
        let mut hary = tfs.open("hary.jpg")?;
        let mut cat = Vec::new();
        while let Some(byte) = hary.read_byte()? {
            cat.push(byte);
        }

        println!("printing hary.jpg");
        let img = image::load_from_memory(&cat)?;
        let resized_image = imageops::resize(&img.to_rgb8(), 60, 30, image::imageops::Nearest);
        let color_support = supports_color::on(Stream::Stdout);
        println!("{}", to_ascii(&resized_image, color_support));

        ls(&tfs)?;

        // also try to open the file but it might not work
        // let mut tmp = NamedTempFile::new()?;
        // tmp.write_all(&cat)?;
        // let _ = open::that(tmp.path());
    }
    Ok(())
}
