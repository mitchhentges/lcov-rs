extern crate byteorder;

use byteorder::{LittleEndian, ByteOrder};
use std::collections::HashMap;
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
        let graph_data = read_gcno(gcno_path);
        read_gcda(gcda_path, graph_data, "/home/mitch/lcov-rs-out");
    } else {
        println!("Usage: lcov-rs GCDA_PATH GCNO_PATH");
    }
}

/// Returns a Vec<FunctionRecord> sorted by identifier
fn read_gcno(gcno_path: &str) -> GraphData {
    println!("Opening gcno file: {}", &gcno_path);
    let path = Path::new(&gcno_path);
    let mut file = match File::open(&path) {
        Err(e) => {
            writeln!(std::io::stderr(), "Failed to open {}:{}", &gcno_path, e.description()).unwrap();
            std::process::exit(1);
        }
        Ok(file) => file
    };

    let mut functions = Vec::<FunctionDefinition>::new();
    let mut blocks = Vec::<BlockRecord>::new();
    let mut lines = Vec::<LineRecord>::new();
    let mut arcs = Vec::<ArcRecord>::new();

    let mut buffer = Vec::<u8>::new();
    file.read_to_end(&mut buffer).unwrap();

    let mut offset = match parse_gcno_header(&buffer) {
        Ok(offset) => offset,
        Err(ParseError { code }) => std::process::exit(code),
    };
    let mut current_function_id = None::<u32>;

    while offset < buffer.len() {
        let tag = LittleEndian::read_u32(&buffer[offset + 0..offset + 4]);
        let length = (LittleEndian::read_u32(&buffer[offset + 4..offset + 8]) * 4) as usize; // file gives length in u32 words

        offset += 8;
        let record_buffer = &buffer[offset..offset+length];

        let record_offset = match tag {
            TAG_FUNCTION => {
                let function_definition = match parse_function_definition(record_buffer) {
                    Ok(tuple) => tuple,
                    Err(ParseError { code }) => std::process::exit(code),
                };
                current_function_id = Some(function_definition.1.identifier);
                functions.push(function_definition.1);
                function_definition.0
            },
            TAG_BLOCKS => {
                let block_records = parse_blocks_record(record_buffer, current_function_id.unwrap());
                blocks.extend(block_records.1);
                block_records.0
            }
            TAG_ARCS => {
                let arc_records = parse_arcs_record(record_buffer, current_function_id.unwrap());
                arcs.extend(arc_records.1);
                arc_records.0
            }
            TAG_LINES => {
                let line_records = match parse_lines_record(record_buffer, current_function_id.unwrap()) {
                    Ok(tuple) => tuple,
                    Err(ParseError { code }) => std::process::exit(code),
                };
                lines.extend(line_records.1);
                line_records.0
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

    return GraphData {
        functions: functions,
        blocks: blocks,
        arcs: arcs,
        lines: lines,
    }
}

fn read_gcda(gcda_path: &str, graph_data: GraphData, tmp_output_path: &str) {
    println!("Opening gcda file: {}", &gcda_path);
    let path = Path::new(&gcda_path);
    let mut file = match File::open(&path) {
        Err(e) => {
            writeln!(std::io::stderr(), "Failed to open {}:{}", &gcda_path, e.description()).unwrap();
            std::process::exit(1);
        }
        Ok(file) => file
    };

    let mut output_file = File::create(Path::new(tmp_output_path)).unwrap();

    let mut buffer = Vec::<u8>::new();
    file.read_to_end(&mut buffer).unwrap();

    let mut offset = match parse_gcda_header(&buffer) {
        Ok(offset) => offset,
        Err(ParseError { code }) => std::process::exit(code),
    };

    output_file.write_all(b"TN:\n");
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
                let function_reference = parse_function_reference(record_buffer);
                current_function_id = Some(function_reference.1.identifier);
                function_reference.0
            },
            TAG_COUNTS => {
                println!(">> TAG_COUNTS");
                let counts_record = parse_counts_record(record_buffer, current_function_id.unwrap());
                counts_records.push(counts_record.1);
                counts_record.0
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

    let mut source_files: Vec<String> = (*graph_data.lines).into_iter()
            .map(|x| x.src_path.clone())
            .collect();
    source_files.sort();
    source_files.dedup();
    for src in source_files {
        print_lcov_output(src, &graph_data, &mut output_file);
    }
}

fn print_lcov_output<T>(src_path: String, graph_data: &GraphData, output_file: &mut T)
    where T: Write {
    output_file.write_all(("file:".to_string() + src_path.as_str() + "\n").as_bytes());

    for function in (*graph_data.functions).iter()
            .filter(|x| x.src_path == src_path) {
        output_file.write_all(("function:".to_string() + "0" + "," + function.name.as_str() + "\n").as_bytes());
        for block in (*graph_data.blocks).iter()
            .filter(|x| x.function_id == function.identifier) {

        }
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
fn parse_function_definition(buffer: &[u8]) -> Result<(usize, FunctionDefinition), ParseError> {
    let identifier = LittleEndian::read_u32(&buffer[0..4]);
    let line_number_checksum = LittleEndian::read_u32(&buffer[4..8]);
    let config_checksum = LittleEndian::read_u32(&buffer[8..12]);
    let name_length = (LittleEndian::read_u32(&buffer[12..16]) * 4) as usize;
    let name = read_utf8(&buffer[16..16 + name_length])?;
    let src_path_length = (LittleEndian::read_u32(&buffer[16 + name_length..20 + name_length]) * 4) as usize;
    let src_path = read_utf8(&buffer[20 + name_length..20 + name_length + src_path_length])?;
    let line_number = LittleEndian::read_u32(&buffer[20 + name_length + src_path_length..24 + name_length + src_path_length]);

    let function_record = FunctionDefinition {
        identifier: identifier,
        line_number_checksum: line_number_checksum,
        config_checksum: config_checksum,
        src_path: src_path.to_owned(),
        name: name.to_owned(),
        line_number: line_number,
    };

    println!("FunctionRecord = {:#?}", function_record);

    return Ok((24 + name_length + src_path_length, function_record));
}

fn parse_counts_record(buffer: &[u8], function_id: u32) -> (usize, CountsRecord) {
    let counts = buffer.chunks(8)
        .map(|x| LittleEndian::read_u64(x))
        .collect::<Vec<u64>>();

    let counts_record = CountsRecord {
        function_id: function_id,
        counts: counts,
    };
    println!("Counts = {:#?}", counts_record);
    return (buffer.len(), counts_record);
}

fn parse_blocks_record(buffer: &[u8], function_id: u32) -> (usize, Vec<BlockRecord>) {
    let block_records = buffer.chunks(4)
            .enumerate()
            .map(|x| BlockRecord{
                index: x.0 as u32,
                function_id: function_id
                // Ignore block "flag", we don't need it
            }).collect::<Vec<BlockRecord>>();
    println!("Blocks = {:#?}", block_records);
    return (buffer.len(), block_records);

}

/// Return tuple of number of bytes parsed (for validation), as well as the LineRecord data
/// TODO instead of using src_path to uniquely identify source file, normalize to a numerical id
fn parse_lines_record(buffer: &[u8], function_id: u32)
    -> Result<(usize, Vec<LineRecord>), ParseError> {
    let block = LittleEndian::read_u32(&buffer[0..4]);

    let mut current_src_path = None::<String>;
    let mut line_offset = 4;
    let mut line_records = Vec::<LineRecord>::new();

    loop {
        let line_no = LittleEndian::read_u32(&buffer[0+line_offset..4+line_offset]);
        println!("line_no: {}", line_no);

        if line_no == 0 { // new filename
            let src_path_length = (LittleEndian::read_u32(&buffer[4+line_offset..8+line_offset]) * 4) as usize;
            // End of lines record
            if src_path_length == 0 {
                println!("We done here");
                line_offset += 8;
                break;
            }

            let src_path = read_utf8(&buffer[8+line_offset..8+line_offset + src_path_length])?;
            println!("src_path: {}", src_path);
            current_src_path = Some(src_path.to_string());
            line_offset += 8 + src_path_length;
        } else {
            line_records.push(LineRecord {
                function_id: function_id,
                src_path: current_src_path.clone().unwrap(),
                block: block,
                line_number: line_no
            });
            line_offset += 4;
        }
    }
    println!("Lines {:#?}", line_records);
    return Ok((line_offset, line_records));
}

/// Requires function_id to be passed in, because it cannot be inferred from the record.
/// gcno files expect the "current function state" to be manually managed
fn parse_arcs_record(buffer: &[u8], function_id: u32) -> (usize, Vec<ArcRecord>) {
    let source_block = LittleEndian::read_u32(&buffer[0..4]);
    let record_count = (buffer.len() - 4) // buffer without source_block
        / 8; // Divide by size of each chunk of data (4byte destination, 4byte flags)

    let mut arc_offset = 4;
    let mut arc_records = Vec::<ArcRecord>::with_capacity(record_count);

    while arc_offset < buffer.len() {
        let destination_block = LittleEndian::read_u32(&buffer[0+arc_offset..4+arc_offset]);
        let flags = LittleEndian::read_u32(&buffer[4+arc_offset..8+arc_offset]);
        arc_offset += 8;
        arc_records.push(ArcRecord {
            function_id: function_id,
            source_block: source_block,
            destination_block: destination_block,
            flags: flags
        });
    };

    println!("Arcs = {:#?}", arc_records);
    return (buffer.len(), arc_records);
}

fn parse_function_reference(buffer: &[u8]) -> (usize, FunctionReference) {
    let identifier = LittleEndian::read_u32(&buffer[0..4]);
    let line_number_checksum = LittleEndian::read_u32(&buffer[4..8]);
    let config_checksum = LittleEndian::read_u32(&buffer[8..12]);

    let function_reference = FunctionReference {
        identifier: identifier,
        line_number_checksum: line_number_checksum,
        config_checksum: config_checksum,
    };
    println!("FunctionReference = {:#?}", function_reference);
    return (12, function_reference);
}

fn read_utf8(buffer: &[u8]) -> Result<&str, str::Utf8Error>  {
    let mut content_end = buffer.len() - 1;
    while buffer[content_end] == 0u8 {
        content_end -= 1;
    }

    return str::from_utf8(&buffer[0..content_end+1]);
}

struct GraphData {
    functions: Vec<FunctionDefinition>,
    blocks: Vec<BlockRecord>,
    lines: Vec<LineRecord>,
    arcs: Vec<ArcRecord>
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

#[derive(Debug)]
struct FunctionDefinition {
    identifier: u32,
    line_number_checksum: u32,
    config_checksum: u32,
    src_path: String,
    name: String,
    line_number: u32,
}

#[derive(Debug)]
struct BlockRecord {
    index: u32,
    function_id: u32,
}

#[derive(Debug)]
struct LineRecord {
    function_id: u32,
    src_path: String,
    block: u32,
    line_number: u32,
}

#[derive(Debug)]
struct ArcRecord {
    function_id: u32,
    source_block: u32,
    destination_block: u32,
    flags: u32,
}

impl ArcRecord {
    fn is_on_tree(&self) -> bool {
        return self.flags & ARC_ON_TREE > 0;
    }
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