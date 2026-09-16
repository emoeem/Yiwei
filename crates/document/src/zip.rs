//! ZIP container reader with the safety limits required by §23.2.
//!
//! EPUB is a ZIP file. A hostile book can therefore be a zip bomb, a path
//! traversal attempt or a container with millions of entries, so every limit is
//! enforced *before* memory is allocated: entry count, per-entry size, total
//! size, compression ratio, and the entry name is validated against traversal.

use std::io::{Read, Seek, SeekFrom};

/// Failure modes when reading a ZIP container.
#[derive(Debug, thiserror::Error)]
pub enum ZipError {
    /// Underlying IO failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// The end-of-central-directory record was not found.
    #[error("not a zip container: end of central directory not found")]
    NotAZip,
    /// The container is structurally broken.
    #[error("malformed zip: {0}")]
    Malformed(String),
    /// The container uses a feature we refuse to guess about.
    #[error("unsupported zip feature: {0}")]
    Unsupported(String),
    /// A safety limit was exceeded.
    #[error("zip limit exceeded: {0}")]
    LimitExceeded(String),
    /// An entry name tried to escape the archive root.
    #[error("unsafe entry name: {0:?}")]
    UnsafeName(String),
    /// The entry used a compression method we do not implement.
    #[error("unsupported compression method {0} for entry {1:?}")]
    UnsupportedMethod(u16, String),
    /// The decompressed size or CRC did not match the directory.
    #[error("integrity check failed for {name:?}: {detail}")]
    Integrity {
        /// Entry name.
        name: String,
        /// What did not match.
        detail: String,
    },
}

/// Safety limits applied while opening and reading a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZipLimits {
    /// Maximum number of entries.
    pub max_entries: usize,
    /// Maximum uncompressed size of a single entry.
    pub max_entry_size: u64,
    /// Maximum total uncompressed size of the archive.
    pub max_total_size: u64,
    /// Maximum accepted compressed:uncompressed ratio per entry.
    pub max_compression_ratio: u64,
}

impl Default for ZipLimits {
    fn default() -> Self {
        Self {
            max_entries: 10_000,
            max_entry_size: 64 * 1024 * 1024,
            max_total_size: 512 * 1024 * 1024,
            max_compression_ratio: 200,
        }
    }
}

/// Compression method of an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// No compression.
    Stored,
    /// Raw deflate.
    Deflate,
}

/// One central directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntry {
    /// Entry path inside the container.
    pub name: String,
    /// Compression method.
    pub method: Method,
    /// Size of the stored bytes.
    pub compressed_size: u64,
    /// Size of the expanded bytes.
    pub uncompressed_size: u64,
    /// CRC-32 of the expanded bytes.
    pub crc32: u32,
    /// Offset of the local header.
    pub local_header_offset: u64,
}

/// A readable ZIP container.
#[derive(Debug)]
pub struct ZipArchive<R> {
    reader: R,
    entries: Vec<ZipEntry>,
    limits: ZipLimits,
}

impl<R: Read + Seek> ZipArchive<R> {
    /// Open a container with default limits.
    pub fn open(reader: R) -> Result<Self, ZipError> {
        Self::with_limits(reader, ZipLimits::default())
    }

    /// Open a container with explicit limits.
    pub fn with_limits(mut reader: R, limits: ZipLimits) -> Result<Self, ZipError> {
        let end = find_end_of_central_directory(&mut reader)?;
        let total_entries = u64::from(end.total_entries);
        if total_entries as usize > limits.max_entries {
            return Err(ZipError::LimitExceeded(format!(
                "{} entries, limit {}",
                total_entries, limits.max_entries
            )));
        }
        if end.has_zip64_marker() {
            return Err(ZipError::Unsupported(
                "zip64 containers are not supported yet".into(),
            ));
        }

        reader.seek(SeekFrom::Start(u64::from(end.central_directory_offset)))?;
        let directory_length = end.central_directory_size as usize;
        let mut directory = vec![0u8; directory_length];
        reader.read_exact(&mut directory)?;

        let entries = parse_central_directory(&directory, total_entries, &limits)?;
        let total: u64 = entries.iter().map(|entry| entry.uncompressed_size).sum();
        if total > limits.max_total_size {
            return Err(ZipError::LimitExceeded(format!(
                "total uncompressed size {total}, limit {}",
                limits.max_total_size
            )));
        }
        for entry in &entries {
            if entry.uncompressed_size > limits.max_entry_size {
                return Err(ZipError::LimitExceeded(format!(
                    "entry {:?} is {} bytes, limit {}",
                    entry.name, entry.uncompressed_size, limits.max_entry_size
                )));
            }
            if entry.method == Method::Deflate {
                let ratio = entry.uncompressed_size / entry.compressed_size.max(1);
                if ratio > limits.max_compression_ratio {
                    return Err(ZipError::LimitExceeded(format!(
                        "entry {:?} has compression ratio {ratio}, limit {}",
                        entry.name, limits.max_compression_ratio
                    )));
                }
            }
        }

        Ok(Self {
            reader,
            entries,
            limits,
        })
    }

    /// All entries.
    pub fn entries(&self) -> &[ZipEntry] {
        &self.entries
    }

    /// The limits in force.
    pub fn limits(&self) -> ZipLimits {
        self.limits
    }

    /// Find an entry by exact name.
    pub fn find(&self, name: &str) -> Option<&ZipEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// Stream one entry through `sink`, returning the number of bytes expanded.
    ///
    /// Bounded memory: the entry is never fully buffered here, which is what
    /// keeps a 100 MB resource from becoming a 100 MB allocation.
    pub fn stream_entry<F>(&mut self, entry: &ZipEntry, mut sink: F) -> Result<u64, ZipError>
    where
        F: FnMut(&[u8]) -> Result<(), ZipError>,
    {
        let data_offset = local_data_offset(&mut self.reader, entry)?;
        self.reader.seek(SeekFrom::Start(data_offset))?;

        let mut hasher = Crc32::new();
        let mut produced: u64 = 0;
        let mut buffer = vec![0u8; 64 * 1024];
        let limit = self.limits.max_entry_size.min(entry.uncompressed_size);

        let mut emit = |chunk: &[u8], produced: &mut u64| -> Result<(), ZipError> {
            *produced += chunk.len() as u64;
            if *produced > limit {
                return Err(ZipError::Integrity {
                    name: entry.name.clone(),
                    detail: format!("expanded beyond the declared size of {limit} bytes"),
                });
            }
            sink(chunk)
        };

        match entry.method {
            Method::Stored => {
                let mut remaining = entry.compressed_size;
                while remaining > 0 {
                    let want = remaining.min(buffer.len() as u64) as usize;
                    self.reader.read_exact(&mut buffer[..want])?;
                    hasher.update(&buffer[..want]);
                    emit(&buffer[..want], &mut produced)?;
                    remaining -= want as u64;
                }
            }
            Method::Deflate => {
                let mut limited = (&mut self.reader).take(entry.compressed_size);
                let mut decoder = flate2::read::DeflateDecoder::new(&mut limited);
                loop {
                    let read = decoder.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..read]);
                    emit(&buffer[..read], &mut produced)?;
                }
            }
        }

        if produced != entry.uncompressed_size {
            return Err(ZipError::Integrity {
                name: entry.name.clone(),
                detail: format!(
                    "expanded to {produced} bytes but the directory declares {}",
                    entry.uncompressed_size
                ),
            });
        }
        if hasher.finish() != entry.crc32 {
            return Err(ZipError::Integrity {
                name: entry.name.clone(),
                detail: "CRC-32 mismatch".into(),
            });
        }
        Ok(produced)
    }

    /// Read one entry into memory (bounded by the entry size limit).
    pub fn read_entry(&mut self, entry: &ZipEntry) -> Result<Vec<u8>, ZipError> {
        let mut out = Vec::with_capacity(entry.uncompressed_size.min(1 << 20) as usize);
        let name = entry.name.clone();
        let entry = entry.clone();
        let bytes = self.stream_entry(&entry, |chunk| {
            out.extend_from_slice(chunk);
            Ok(())
        })?;
        debug_assert_eq!(bytes, out.len() as u64, "streamed {bytes} into {name}");
        Ok(out)
    }

    /// Read an entry by name.
    pub fn read_path(&mut self, name: &str) -> Result<Vec<u8>, ZipError> {
        let entry = self
            .find(name)
            .cloned()
            .ok_or_else(|| ZipError::Malformed(format!("entry {name:?} not found")))?;
        self.read_entry(&entry)
    }
}

fn local_data_offset<R: Read + Seek>(
    reader: &mut R,
    entry: &ZipEntry,
) -> Result<u64, ZipError> {
    reader.seek(SeekFrom::Start(entry.local_header_offset))?;
    let mut header = [0u8; 30];
    reader.read_exact(&mut header)?;
    if u32::from_le_bytes([header[0], header[1], header[2], header[3]]) != LOCAL_HEADER_SIGNATURE {
        return Err(ZipError::Malformed(format!(
            "entry {:?} has no local header at offset {}",
            entry.name, entry.local_header_offset
        )));
    }
    let name_length = u16::from_le_bytes([header[26], header[27]]) as u64;
    let extra_length = u16::from_le_bytes([header[28], header[29]]) as u64;
    Ok(entry.local_header_offset + 30 + name_length + extra_length)
}

const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;
const END_OF_CENTRAL_DIRECTORY_SIGNATURE: u32 = 0x0605_4b50;

struct EndOfCentralDirectory {
    total_entries: u16,
    central_directory_size: u32,
    central_directory_offset: u32,
}

impl EndOfCentralDirectory {
    fn has_zip64_marker(&self) -> bool {
        self.total_entries == u16::MAX
            || self.central_directory_offset == u32::MAX
            || self.central_directory_size == u32::MAX
    }
}

fn find_end_of_central_directory<R: Read + Seek>(
    reader: &mut R,
) -> Result<EndOfCentralDirectory, ZipError> {
    let length = reader.seek(SeekFrom::End(0))?;
    if length < 22 {
        return Err(ZipError::NotAZip);
    }
    let window = length.min(66_000);
    reader.seek(SeekFrom::Start(length - window))?;
    let mut buffer = vec![0u8; window as usize];
    reader.read_exact(&mut buffer)?;

    for offset in (0..buffer.len().saturating_sub(21)).rev() {
        if u32::from_le_bytes([
            buffer[offset],
            buffer[offset + 1],
            buffer[offset + 2],
            buffer[offset + 3],
        ]) != END_OF_CENTRAL_DIRECTORY_SIGNATURE
        {
            continue;
        }
        let comment_length =
            u16::from_le_bytes([buffer[offset + 20], buffer[offset + 21]]) as usize;
        if offset + 22 + comment_length != buffer.len() {
            continue;
        }
        return Ok(EndOfCentralDirectory {
            total_entries: u16::from_le_bytes([buffer[offset + 10], buffer[offset + 11]]),
            central_directory_size: u32::from_le_bytes([
                buffer[offset + 12],
                buffer[offset + 13],
                buffer[offset + 14],
                buffer[offset + 15],
            ]),
            central_directory_offset: u32::from_le_bytes([
                buffer[offset + 16],
                buffer[offset + 17],
                buffer[offset + 18],
                buffer[offset + 19],
            ]),
        });
    }
    Err(ZipError::NotAZip)
}

fn parse_central_directory(
    directory: &[u8],
    expected_entries: u64,
    limits: &ZipLimits,
) -> Result<Vec<ZipEntry>, ZipError> {
    let mut entries = Vec::new();
    let mut cursor = 0usize;
    while cursor + 46 <= directory.len() {
        let header = &directory[cursor..];
        let signature = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        if signature != CENTRAL_HEADER_SIGNATURE {
            return Err(ZipError::Malformed(format!(
                "unexpected central directory signature at offset {cursor}"
            )));
        }
        let flags = u16::from_le_bytes([header[8], header[9]]);
        if flags & 0x0001 != 0 {
            return Err(ZipError::Unsupported("encrypted entry".into()));
        }
        let method = u16::from_le_bytes([header[10], header[11]]);
        let method = match method {
            0 => Method::Stored,
            8 => Method::Deflate,
            other => {
                let name_length = u16::from_le_bytes([header[28], header[29]]) as usize;
                let name = String::from_utf8_lossy(&header[46..46 + name_length]).to_string();
                return Err(ZipError::UnsupportedMethod(other, name));
            }
        };
        let crc32 = u32::from_le_bytes([header[16], header[17], header[18], header[19]]);
        let compressed_size =
            u32::from_le_bytes([header[20], header[21], header[22], header[23]]) as u64;
        let uncompressed_size =
            u32::from_le_bytes([header[24], header[25], header[26], header[27]]) as u64;
        let name_length = u16::from_le_bytes([header[28], header[29]]) as usize;
        let extra_length = u16::from_le_bytes([header[30], header[31]]) as usize;
        let comment_length = u16::from_le_bytes([header[32], header[33]]) as usize;
        let local_header_offset =
            u32::from_le_bytes([header[42], header[43], header[44], header[45]]) as u64;

        let name_start = cursor + 46;
        let name_end = name_start + name_length;
        if name_end > directory.len() {
            return Err(ZipError::Malformed("entry name runs past the directory".into()));
        }
        let name = String::from_utf8_lossy(&directory[name_start..name_end]).to_string();
        validate_entry_name(&name)?;

        entries.push(ZipEntry {
            name,
            method,
            compressed_size,
            uncompressed_size,
            crc32,
            local_header_offset,
        });
        cursor = name_end + extra_length + comment_length;
    }

    if entries.len() as u64 != expected_entries {
        return Err(ZipError::Malformed(format!(
            "central directory holds {} entries but the trailer declares {expected_entries}",
            entries.len()
        )));
    }
    if entries.len() > limits.max_entries {
        return Err(ZipError::LimitExceeded(format!(
            "{} entries, limit {}",
            entries.len(),
            limits.max_entries
        )));
    }
    Ok(entries)
}

/// Reject names that could escape the archive root (§23.2).
fn validate_entry_name(name: &str) -> Result<(), ZipError> {
    if name.is_empty() {
        return Err(ZipError::UnsafeName(name.to_string()));
    }
    if name.starts_with('/') || name.starts_with('\\') || name.contains('\\') {
        return Err(ZipError::UnsafeName(name.to_string()));
    }
    if name.contains(':') {
        return Err(ZipError::UnsafeName(name.to_string()));
    }
    for segment in name.split('/') {
        if segment == ".." {
            return Err(ZipError::UnsafeName(name.to_string()));
        }
    }
    Ok(())
}

/// CRC-32 (IEEE 802.3), used to verify expanded entries.
#[derive(Debug, Clone)]
pub(crate) struct Crc32 {
    value: u32,
}

impl Crc32 {
    pub(crate) fn new() -> Self {
        Self { value: 0xffff_ffff }
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            let index = ((self.value ^ u32::from(*byte)) & 0xff) as usize;
            self.value = (self.value >> 8) ^ CRC_TABLE[index];
        }
    }

    pub(crate) fn finish(self) -> u32 {
        self.value ^ 0xffff_ffff
    }

    /// CRC-32 of a complete buffer, for writers.
    pub(crate) fn of(bytes: &[u8]) -> u32 {
        let mut crc = Self::new();
        crc.update(bytes);
        crc.finish()
    }
}

const CRC_TABLE: [u32; 256] = build_crc_table();

const fn build_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0usize;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 1 {
                0xedb8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
}
