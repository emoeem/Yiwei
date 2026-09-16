//! Minimal ZIP writer.
//!
//! It exists so that tests, the corpus generator and the export path can build
//! real EPUB/`ZIP` containers without pulling in a third-party archive crate. It
//! writes standard local headers, a central directory and an end-of-central
//! directory record, with `stored` and `deflate` entries.

use std::io::Write;

use crate::zip::{Crc32, Method};

/// Signature of a local file header.
const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
/// Signature of a central directory header.
const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;
/// Signature of the end of central directory record.
const END_OF_CENTRAL_DIRECTORY_SIGNATURE: u32 = 0x0605_4b50;
/// DOS date for 1980-01-01, the earliest representable date.
const DOS_DATE: u16 = 0x0021;

#[derive(Debug)]
struct CentralEntry {
    name: String,
    method: u16,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    offset: u32,
}

/// Streaming ZIP writer.
#[derive(Debug)]
pub struct ZipWriter<W: Write> {
    writer: W,
    entries: Vec<CentralEntry>,
    offset: u64,
    finished: bool,
}

impl<W: Write> ZipWriter<W> {
    /// Create a writer over `writer`.
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            entries: Vec::new(),
            offset: 0,
            finished: false,
        }
    }

    /// Add an entry stored without compression.
    pub fn add_stored(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        self.add_entry(name, data, Method::Stored)
    }

    /// Add an entry compressed with raw deflate.
    pub fn add_deflated(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        self.add_entry(name, data, Method::Deflate)
    }

    /// Write the central directory and return the inner writer.
    pub fn finish(mut self) -> std::io::Result<W> {
        let directory_offset = self.offset;
        let entries = std::mem::take(&mut self.entries);
        for entry in &entries {
            let mut header = Vec::with_capacity(46 + entry.name.len());
            header.extend_from_slice(&CENTRAL_HEADER_SIGNATURE.to_le_bytes());
            header.extend_from_slice(&20u16.to_le_bytes()); // version made by
            header.extend_from_slice(&20u16.to_le_bytes()); // version needed
            header.extend_from_slice(&0u16.to_le_bytes()); // flags
            header.extend_from_slice(&entry.method.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes()); // time
            header.extend_from_slice(&DOS_DATE.to_le_bytes());
            header.extend_from_slice(&entry.crc32.to_le_bytes());
            header.extend_from_slice(&entry.compressed_size.to_le_bytes());
            header.extend_from_slice(&entry.uncompressed_size.to_le_bytes());
            header.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes()); // extra
            header.extend_from_slice(&0u16.to_le_bytes()); // comment
            header.extend_from_slice(&0u16.to_le_bytes()); // disk
            header.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            header.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            header.extend_from_slice(&entry.offset.to_le_bytes());
            header.extend_from_slice(entry.name.as_bytes());
            self.writer.write_all(&header)?;
            self.offset += header.len() as u64;
        }
        let directory_size = self.offset - directory_offset;

        let mut trailer = Vec::with_capacity(22);
        trailer.extend_from_slice(&END_OF_CENTRAL_DIRECTORY_SIGNATURE.to_le_bytes());
        trailer.extend_from_slice(&0u16.to_le_bytes());
        trailer.extend_from_slice(&0u16.to_le_bytes());
        trailer.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        trailer.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        trailer.extend_from_slice(&(directory_size as u32).to_le_bytes());
        trailer.extend_from_slice(&(directory_offset as u32).to_le_bytes());
        trailer.extend_from_slice(&0u16.to_le_bytes());
        self.writer.write_all(&trailer)?;
        self.finished = true;
        self.writer.flush()?;
        Ok(self.writer)
    }

    fn add_entry(&mut self, name: &str, data: &[u8], method: Method) -> std::io::Result<()> {
        assert!(!self.finished, "zip writer already finished");
        let payload = match method {
            Method::Stored => data.to_vec(),
            Method::Deflate => {
                let mut encoder = flate2::write::DeflateEncoder::new(
                    Vec::new(),
                    flate2::Compression::default(),
                );
                encoder.write_all(data)?;
                encoder.finish()?
            }
        };
        let crc32 = Crc32::of(data);
        let header_offset = self.offset as u32;

        let mut header = Vec::with_capacity(30 + name.len());
        header.extend_from_slice(&LOCAL_HEADER_SIGNATURE.to_le_bytes());
        header.extend_from_slice(&20u16.to_le_bytes());
        header.extend_from_slice(&0u16.to_le_bytes());
        header.extend_from_slice(&method_code(method).to_le_bytes());
        header.extend_from_slice(&0u16.to_le_bytes());
        header.extend_from_slice(&DOS_DATE.to_le_bytes());
        header.extend_from_slice(&crc32.to_le_bytes());
        header.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        header.extend_from_slice(&(data.len() as u32).to_le_bytes());
        header.extend_from_slice(&(name.len() as u16).to_le_bytes());
        header.extend_from_slice(&0u16.to_le_bytes());
        header.extend_from_slice(name.as_bytes());

        self.writer.write_all(&header)?;
        self.writer.write_all(&payload)?;
        self.offset += (header.len() + payload.len()) as u64;

        self.entries.push(CentralEntry {
            name: name.to_string(),
            method: method_code(method),
            crc32,
            compressed_size: payload.len() as u32,
            uncompressed_size: data.len() as u32,
            offset: header_offset,
        });
        Ok(())
    }
}

fn method_code(method: Method) -> u16 {
    match method {
        Method::Stored => 0,
        Method::Deflate => 8,
    }
}
