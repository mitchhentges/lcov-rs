extern crate byteorder;

use byteorder::{LittleEndian, ByteOrder};
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

const GCNO_MAGIC: u32 = 0x67636e6f;

fn main() {
    let args: Vec<String> = env::args().collect();

    if let Some(gcno_path) = args.get(1) {
        println!("Opening gcno file: {}", &gcno_path);
        let path = Path::new(&gcno_path);
        let mut file = match File::open(&path) {
            Err(e) => {
                writeln!(std::io::stderr(), "Failed to open {}:{}", &gcno_path, e.description()).unwrap();
                return;
            }
            Ok(file) => file
        };

        let mut buffer = Vec::<u8>::new();
        file.read_to_end(&mut buffer).unwrap();
        let magic = LittleEndian::read_u32(&buffer[0..4]);
        println!("{:X}, {:X}, {}", magic, GCNO_MAGIC, magic == GCNO_MAGIC)
    } else {
        println!("Usage: bohemian-waxwing PATH_TO_GCNO");
    }
}
