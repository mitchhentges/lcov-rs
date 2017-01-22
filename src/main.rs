extern crate byteorder;

use byteorder::{LittleEndian, ByteOrder};
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::str;

const GCNO_MAGIC: u32 = 0x67636e6f;

fn main() {
    let args: Vec<String> = env::args().collect();

    if let Some(gcno_path) = args.get(1) {
        println!("Opening gcno file: {}", &gcno_path);
        let path = Path::new(&gcno_path);
        let mut file = match File::open(&path) {
            Err(e) => {
                writeln!(std::io::stderr(), "Failed to open {}:{}", &gcno_path, e.description()).unwrap();
                std::process::exit(1);
            }
            Ok(file) => file
        };

        let mut buffer = Vec::<u8>::new();
        file.read_to_end(&mut buffer).unwrap();
        if let Err(code) = parse_header(&buffer) {
            std::process::exit(code);
        }

        let offset = 12u8; // Start after magic, version, stamp

    } else {
        println!("Usage: bohemian-waxwing PATH_TO_GCNO");
    }
}

fn parse_header(buffer: &Vec<u8>) -> Result<(), i32> {
    if GCNO_MAGIC != LittleEndian::read_u32(&buffer[0..4]) {
        return Err(2);
    };

    if let Ok(ref version) = str::from_utf8(&buffer[4..8]) {
        println!("version: {}", version);
    };

    return Ok(());
}