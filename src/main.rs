extern crate byteorder;
extern crate itertools;

use byteorder::{LittleEndian, ByteOrder};
use itertools::Itertools;
use std::convert::From;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::str;

const GCDA_MAGIC: u32 = 0x67636461;
const GCNO_MAGIC: u32 = 0x67636e6f;
const TAG_FUNCTION: u32 = 0x01000000;
const TAG_COUNTS: u32 = 0x01a10000; // For MVP, ignore multi-run values
const TAG_BLOCKS: u32 = 0x01410000;
const TAG_ARCS: u32 = 0x01430000;
const TAG_LINES: u32 = 0x01450000;
const TAG_END_FILE: u32 = 0x00000000;

const ARC_ON_TREE: u32 = 1 << 0;

fn main() {
    let args: Vec<String> = env::args().collect();

    if let (Some(gcda_path), Some(gcno_path)) = (args.get(1), args.get(2)) {
        let file_notes = read_gcno(gcno_path);
        println!("file_notes = {:#?}", file_notes);
        read_gcda(gcda_path, "/home/mitch/lcov-rs-out");
    } else {
        println!("Usage: lcov-rs GCDA_PATH GCNO_PATH");
    }
}

/// Returns a Vec<FunctionRecord> sorted by identifier
fn read_gcno(gcno_path: &str) -> Vec<FileNotes> {
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

    let mut offset = match parse_gcno_header(&buffer) {
        Ok(offset) => offset,
        Err(ParseError { code }) => std::process::exit(code),
    };

    let mut functions: Vec<FunctionNotes> = Vec::new();

    while offset < buffer.len() {
        let tag = LittleEndian::read_u32(&buffer[offset + 0..offset + 4]);
        let length = (LittleEndian::read_u32(&buffer[offset + 4..offset + 8]) * 4) as usize; // file gives length in u32 words

        offset += 8;
        let record_buffer = &buffer[offset..offset+length];

        let record_offset = match tag {
            TAG_FUNCTION => {
                let parsed = match parse_function_definition(record_buffer) {
                    Ok(tuple) => tuple,
                    Err(ParseError { code }) => std::process::exit(code),
                };
                let record = parsed.record;
                functions.push(FunctionNotes {
                    identifier: record.identifier,
                    line_number_checksum: record.line_number_checksum,
                    config_checksum: record.config_checksum,
                    src_path: record.src_path,
                    name: record.name,
                    line_number: record.line_number,
                    blocks: Vec::new()
                });
                parsed.length
            },
            TAG_BLOCKS => {
                let parsed = parse_blocks_record(record_buffer);
                if let Some(ref mut function) = functions.last_mut() {
                    function.blocks.push(BlockNotes {
                        line_number: None, // TODO deal with this better without mutability
                        arcs: Vec::new()
                    });
                }
                parsed.length
            }
            TAG_ARCS => {
                let parsed = parse_arcs_record(record_buffer);
                if let Some(ref mut function) = functions.last_mut() {
                    let record = parsed.record;
                    if let Some(ref mut block) = function.blocks.get_mut(record.source_block as usize) {
                        for arc in record.arcs {
                            block.arcs.push(ArcNotes {
                                destination_block: arc.destination_block,
                                flags: arc.flags
                            });
                        }
                    }
                }
                parsed.length
            }
            TAG_LINES => {
                let parsed = match parse_lines_record(record_buffer) {
                    Ok(tuple) => tuple,
                    Err(ParseError { code }) => std::process::exit(code),
                };
                if let Some(ref mut function) = functions.last_mut() {
                    for line in parsed.record {
                        if let Some(ref mut block) = function.blocks.get_mut(line.block as usize) {
                            block.line_number = Some(line.line_number);
                        }
                    }
                }
                parsed.length
            },
            TAG_END_FILE => {
                break;
            },
            _ => length, // skip record, it's not useful to us
        };
        if record_offset != length {
            println!("!! record_offset != length [{}|{}]", record_offset, length);
            panic!();
        }
        offset += if record_offset != 0 { record_offset } else { 1 * 4 };
    }

    functions.sort_by_key(|k| k.src_path.clone());
    return functions.into_iter()
            .group_by(|f| f.src_path.clone())
            .into_iter()
            .map(|(k, g)| {
                FileNotes {
                    src_path: k,
                    functions: g.collect()
                }
            })
            .collect();
}

fn read_gcda(gcda_path: &str, tmp_output_path: &str) {
    let path = Path::new(&gcda_path);
    let mut file = match File::open(&path) {
        Err(e) => {
            writeln!(std::io::stderr(), "Failed to open {}:{}", &gcda_path, e.description()).unwrap();
            std::process::exit(1);
        }
        Ok(file) => file
    };

    let mut buffer = Vec::<u8>::new();
    file.read_to_end(&mut buffer).unwrap();

    let mut offset = match parse_gcda_header(&buffer) {
        Ok(offset) => offset,
        Err(ParseError { code }) => std::process::exit(code),
    };

    let mut current_function_id = None::<u32>;
    let mut counts_records = Vec::<CountsRecord>::new();

    while offset < buffer.len() {
        let tag = LittleEndian::read_u32(&buffer[offset + 0..offset + 4]);
        let length = (LittleEndian::read_u32(&buffer[offset + 4..offset + 8]) * 4) as usize; // file gives length in u32 words

        offset += 8;
        let record_buffer = &buffer[offset..offset+length];

        let record_offset = match tag {
            TAG_FUNCTION => {
                println!(">> TAG_FUNCTION");
                let parsed = parse_function_reference(record_buffer);
                current_function_id = Some(parsed.record.identifier);
                parsed.length
            },
            TAG_COUNTS => {
                println!(">> TAG_COUNTS");
                let counts_record = parse_counts_record(record_buffer, current_function_id.unwrap());
                counts_records.push(counts_record.record);
                counts_record.length
            }
            TAG_END_FILE => {
                break;
            },
            _ => length, // skip record, it's not useful to us
        };
        if record_offset != length {
            println!("!! record_offset != length [{}|{}]", record_offset, length);
            panic!();
        }
        offset += if record_offset != 0 { record_offset } else { 1 * 4 };
    }
}

fn parse_gcda_header(buffer: &[u8]) -> Result<usize, ParseError> {
    if GCDA_MAGIC != LittleEndian::read_u32(&buffer[0..4]) {
        writeln!(std::io::stderr(),
                 "Invalid magic bytes. Could be an endian issue if on non-Linux").unwrap();
        return Err(ParseError::new(2));
    };

    println!("version: {}", read_utf8(&buffer[4..8])?);
    return Ok(12); // Read magic, version, skip stamp
}

fn parse_gcno_header(buffer: &[u8]) -> Result<usize, ParseError> {
    if GCNO_MAGIC != LittleEndian::read_u32(&buffer[0..4]) {
        writeln!(std::io::stderr(),
                 "Invalid magic bytes. Could be an endian issue if on non-Linux").unwrap();
        return Err(ParseError::new(2));
    };

    println!("version: {}", read_utf8(&buffer[4..8])?);
    return Ok(12); // Read magic, version, skip stamp
}

/// Return tuple of number of bytes parsed (for validation), as well as the FunctionDefinition data
fn parse_function_definition(buffer: &[u8]) -> Result<Record<FunctionRecord>, ParseError> {
    let identifier = LittleEndian::read_u32(&buffer[0..4]);
    let line_number_checksum = LittleEndian::read_u32(&buffer[4..8]);
    let config_checksum = LittleEndian::read_u32(&buffer[8..12]);
    let name_length = (LittleEndian::read_u32(&buffer[12..16]) * 4) as usize;
    let name = read_utf8(&buffer[16..16 + name_length])?;
    let src_path_length = (LittleEndian::read_u32(&buffer[16 + name_length..20 + name_length]) * 4) as usize;
    let src_path = read_utf8(&buffer[20 + name_length..20 + name_length + src_path_length])?;
    let line_number = LittleEndian::read_u32(&buffer[20 + name_length + src_path_length..24 + name_length + src_path_length]);

    let function_record = FunctionRecord {
        identifier: identifier,
        line_number_checksum: line_number_checksum,
        config_checksum: config_checksum,
        src_path: src_path.to_owned(),
        name: name.to_owned(),
        line_number: line_number,
    };

    println!("FunctionRecord = {:#?}", function_record);
    return Ok(Record {
        length: 24 + name_length + src_path_length,
        record: function_record
    });
}

fn parse_counts_record(buffer: &[u8], function_id: u32) -> Record<CountsRecord> {
    let counts = buffer.chunks(8)
        .map(|x| LittleEndian::read_u64(x))
        .collect::<Vec<u64>>();

    let counts_record = CountsRecord {
        function_id: function_id,
        counts: counts,
    };
    println!("Counts = {:#?}", counts_record);
    return Record {
        length: buffer.len(),
        record: counts_record
    };
}

fn parse_blocks_record(buffer: &[u8]) -> Record<Vec<BlockRecord>> {
    let block_records = buffer.chunks(4)
            .map(|x| BlockRecord{
                flags: LittleEndian::read_u32(x)
            }).collect::<Vec<BlockRecord>>();
    println!("Blocks = {:#?}", block_records);
    return Record {
        length: buffer.len(),
        record: block_records
    };

}

/// Return tuple of number of bytes parsed (for validation), as well as the LineRecord data
/// TODO instead of using src_path to uniquely identify source file, normalize to a numerical id
fn parse_lines_record(buffer: &[u8]) -> Result<Record<Vec<LineRecord>>, ParseError> {
    let block = LittleEndian::read_u32(&buffer[0..4]);

    let mut current_src_path = None::<String>;
    let mut line_offset = 4;
    let mut line_records = Vec::<LineRecord>::new();

    loop {
        let line_no = LittleEndian::read_u32(&buffer[0+line_offset..4+line_offset]);

        if line_no == 0 { // new filename
            let src_path_length = (LittleEndian::read_u32(&buffer[4+line_offset..8+line_offset]) * 4) as usize;

            // End of lines record
            if src_path_length == 0 {
                line_offset += 8;
                break;
            }

            let src_path = read_utf8(&buffer[8+line_offset..8+line_offset + src_path_length])?;
            current_src_path = Some(src_path.to_string());
            line_offset += 8 + src_path_length;
        } else {
            line_records.push(LineRecord {
                src_path: current_src_path.clone().unwrap(),
                block: block,
                line_number: line_no
            });
            line_offset += 4;
        }
    }
    println!("Lines {:#?}", line_records);
    return Ok(Record {
        length: line_offset,
        record: line_records
    });
}

/// Requires function_id to be passed in, because it cannot be inferred from the record.
/// gcno files expect the "current function state" to be manually managed
fn parse_arcs_record(buffer: &[u8]) -> Record<ArcRecord> {
    let source_block = LittleEndian::read_u32(&buffer[0..4]);
    let record_count = (buffer.len() - 4) // buffer without source_block
        / 8; // Divide by size of each chunk of data (4byte destination, 4byte flags)

    let mut arc_offset = 4;
    let mut destinations = Vec::<ArcDestinationRecord>::with_capacity(record_count);

    while arc_offset < buffer.len() {
        let destination_block = LittleEndian::read_u32(&buffer[0+arc_offset..4+arc_offset]);
        let flags = LittleEndian::read_u32(&buffer[4+arc_offset..8+arc_offset]);
        arc_offset += 8;
        destinations.push(ArcDestinationRecord {
            destination_block: destination_block,
            flags: flags
        });
    };

    let arc_record = ArcRecord {
        source_block: source_block,
        arcs: destinations
    };
    println!("Arcs = {:#?}", arc_record);
    return Record {
        length: buffer.len(),
        record: arc_record
    };
}

fn parse_function_reference(buffer: &[u8]) -> Record<FunctionReference> {
    let identifier = LittleEndian::read_u32(&buffer[0..4]);
    let line_number_checksum = LittleEndian::read_u32(&buffer[4..8]);
    let config_checksum = LittleEndian::read_u32(&buffer[8..12]);

    let function_reference = FunctionReference {
        identifier: identifier,
        line_number_checksum: line_number_checksum,
        config_checksum: config_checksum,
    };
    println!("FunctionReference = {:#?}", function_reference);
    return Record {
        length: 12,
        record: function_reference
    };
}

fn read_utf8(buffer: &[u8]) -> Result<&str, str::Utf8Error>  {
    let mut content_end = buffer.len() - 1;
    while buffer[content_end] == 0u8 {
        content_end -= 1;
    }

    return str::from_utf8(&buffer[0..content_end+1]);
}

struct Record<T> {
    length: usize,
    record: T
}

#[derive(Debug)]
struct FileNotes {
    src_path: String,
    functions: Vec<FunctionNotes>
}

#[derive(Debug)]
struct FunctionNotes {
    identifier: u32,
    line_number_checksum: u32,
    config_checksum: u32,
    src_path: String,
    name: String,
    line_number: u32,
    blocks: Vec<BlockNotes>
}

#[derive(Debug)]
struct BlockNotes {
    line_number: Option<u32>,
    arcs: Vec<ArcNotes>
}

#[derive(Debug)]
struct ArcNotes {
    destination_block: u32,
    flags: u32,
}

impl ArcNotes {
    fn is_on_tree(&self) -> bool {
        return self.flags & ARC_ON_TREE > 0;
    }
}

#[derive(Debug)]
struct FunctionReference {
    identifier: u32,
    line_number_checksum: u32,
    config_checksum: u32,
}

/// Represents the amount of times that each respective instrumented arc is executed.
/// There isn't an execution "count" for _every_ arc, just the instrumented (not on-tree) arcs.
/// The remaining arcs' execution counts are resolved afterwards
#[derive(Debug)]
struct CountsRecord {
    function_id: u32,
    counts: Vec<u64>,
}

/// Gcno definition of a function
#[derive(Debug)]
struct FunctionRecord {
    identifier: u32,
    line_number_checksum: u32,
    config_checksum: u32,
    src_path: String,
    name: String,
    line_number: u32,
}

#[derive(Debug)]
struct BlockRecord {
    flags: u32
}

#[derive(Debug)]
struct LineRecord {
    src_path: String,
    block: u32,
    line_number: u32,
}

#[derive(Debug)]
struct ArcRecord {
    source_block: u32,
    arcs: Vec<ArcDestinationRecord>
}

#[derive(Debug)]
struct ArcDestinationRecord {
    destination_block: u32,
    flags: u32,
}

struct ParseError {
    code: i32,
}

impl ParseError {
    fn new(code: i32) -> ParseError {
        return ParseError { code: code };
    }
}

impl From<str::Utf8Error> for ParseError {
    fn from(_: str::Utf8Error) -> ParseError {
        return ParseError { code: 3 };
    }
}