//! EPUB parsing and the derived text index (design report §10.1, §10.2, §10.3).
//!
//! Opening a book reads only `META-INF/container.xml`, the OPF package document
//! and the navigation document. Sections are extracted on demand, so a 100 MB
//! EPUB never loads all of its chapters, and every derived value (block ID,
//! section fingerprint) can be rebuilt from the untouched original file.

use std::collections::BTreeMap;
use std::io::{Read, Seek};
use std::path::Path;

use quick_xml::Reader;
use quick_xml::events::Event;
use reader_annotation::{BlockText, SectionText};
use reader_model::{sha256_hex, spine_item_id, text_block_id};
use serde::{Deserialize, Serialize};

use crate::error::DocumentError;
use crate::xhtml;
use crate::zip::ZipArchive;

/// Path of the EPUB container description.
pub const CONTAINER_PATH: &str = "META-INF/container.xml";

/// Book metadata from the OPF package document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EpubMetadata {
    /// `dc:title`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `dc:creator` entries, in document order.
    #[serde(default)]
    pub authors: Vec<String>,
    /// `dc:language`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// `dc:identifier`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    /// `dc:publisher`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// Any other metadata element, keyed by local name.
    #[serde(default)]
    pub extra: BTreeMap<String, Vec<String>>,
}

/// One manifest item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestItem {
    /// Item ID.
    pub id: String,
    /// Archive-relative href.
    pub href: String,
    /// Media type.
    pub media_type: String,
    /// `properties` tokens (e.g. `nav`, `cover-image`).
    #[serde(default)]
    pub properties: Vec<String>,
}

/// One spine item in reading order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpineItem {
    /// Manifest ID referenced by `itemref`.
    pub idref: String,
    /// Archive-relative href of the content document.
    pub href: String,
    /// Media type of the content document.
    pub media_type: String,
    /// Whether the document is part of the linear reading order.
    pub linear: bool,
    /// Item properties, if any.
    #[serde(default)]
    pub properties: Vec<String>,
    /// Position in the reading order.
    pub ordinal: u32,
}

/// One navigation entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavNode {
    /// Visible label.
    pub label: String,
    /// Target href.
    pub href: String,
    /// Fragment identifier, if the href carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
    /// Nested entries.
    #[serde(default)]
    pub children: Vec<NavNode>,
}

/// Where the navigation tree came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavSource {
    /// EPUB 3 navigation document.
    Nav,
    /// EPUB 2 NCX.
    Ncx,
}

/// Parsed package document plus navigation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpubPackage {
    /// Path of the OPF document inside the container.
    pub opf_path: String,
    /// Book metadata.
    pub metadata: EpubMetadata,
    /// Manifest items.
    pub manifest: Vec<ManifestItem>,
    /// Spine items in reading order.
    pub spine: Vec<SpineItem>,
    /// Navigation tree.
    #[serde(default)]
    pub nav: Vec<NavNode>,
    /// Source of the navigation tree, if one was found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nav_source: Option<NavSource>,
}

impl EpubPackage {
    /// Cover image href, taken from manifest properties.
    pub fn cover_href(&self) -> Option<&str> {
        self.manifest
            .iter()
            .find(|item| item.properties.iter().any(|value| value == "cover-image"))
            .map(|item| item.href.as_str())
    }

    /// Manifest item by ID.
    pub fn item(&self, id: &str) -> Option<&ManifestItem> {
        self.manifest.iter().find(|item| item.id == id)
    }
}

/// An open EPUB file.
#[derive(Debug)]
pub struct Epub<R> {
    archive: ZipArchive<R>,
    package: EpubPackage,
    edition_id: String,
}

impl Epub<std::fs::File> {
    /// Open an EPUB from disk.
    ///
    /// Only the container, the package document and the navigation document are
    /// parsed; sections are read lazily by [`Epub::section`].
    pub fn open(
        path: impl AsRef<Path>,
        edition_id: impl Into<String>,
    ) -> Result<Self, DocumentError> {
        let file = std::fs::File::open(path)?;
        let mut archive = ZipArchive::open(file)?;
        let container = archive.read_path(CONTAINER_PATH)?;
        let opf_path = container_rootfile(&container)?;
        let opf_bytes = archive
            .read_path(&opf_path)
            .map_err(|_| DocumentError::MissingFile(opf_path.clone()))?;
        let opf_text = String::from_utf8_lossy(&opf_bytes).to_string();
        let mut package = parse_package(&opf_text, &opf_path)?;

        let (nav, nav_source) = load_navigation(&mut archive, &package)?;
        package.nav = nav;
        package.nav_source = nav_source;

        Ok(Self {
            archive,
            package,
            edition_id: edition_id.into(),
        })
    }
}

impl<R: Read + Seek> Epub<R> {
    /// Open an EPUB from any `Read + Seek` source (memory buffer, custom file,
    /// WASM stream, …).
    ///
    /// This is the primitive entry point. [`Epub::open`] is a convenience
    /// wrapper that opens a local file and delegates here.
    pub fn open_reader(
        reader: R,
        edition_id: impl Into<String>,
    ) -> Result<Self, DocumentError> {
        let mut archive = ZipArchive::open(reader)?;
        let container = archive.read_path(CONTAINER_PATH)?;
        let opf_path = container_rootfile(&container)?;
        let opf_bytes = archive
            .read_path(&opf_path)
            .map_err(|_| DocumentError::MissingFile(opf_path.clone()))?;
        let opf_text = String::from_utf8_lossy(&opf_bytes).to_string();
        let mut package = parse_package(&opf_text, &opf_path)?;

        let (nav, nav_source) = load_navigation(&mut archive, &package)?;
        package.nav = nav;
        package.nav_source = nav_source;

        Ok(Self {
            archive,
            package,
            edition_id: edition_id.into(),
        })
    }

    /// The parsed package.
    pub fn package(&self) -> &EpubPackage {
        &self.package
    }

    /// Edition ID this index belongs to.
    pub fn edition_id(&self) -> &str {
        &self.edition_id
    }

    /// Extract one section into the derived index.
    pub fn section(&mut self, spine_index: u32) -> Result<SectionText, DocumentError> {
        let item = self
            .package
            .spine
            .get(spine_index as usize)
            .cloned()
            .ok_or_else(|| {
                DocumentError::MissingField(
                    format!("spine[{spine_index}]"),
                    self.package.opf_path.clone(),
                )
            })?;
        let bytes = self
            .archive
            .read_path(&item.href)
            .map_err(|_| DocumentError::MissingFile(item.href.clone()))?;
        let text = String::from_utf8_lossy(&bytes).to_string();
        let blocks = xhtml::extract_blocks(&text)?;
        Ok(build_section(
            &self.edition_id,
            spine_index,
            &item.href,
            &blocks,
        ))
    }

    /// Extract every spine item, up to `max_sections` (0 means "all").
    pub fn index_all(&mut self, max_sections: usize) -> Result<Vec<SectionText>, DocumentError> {
        let count = self.package.spine.len();
        let limit = if max_sections == 0 {
            count
        } else {
            max_sections.min(count)
        };
        let mut sections = Vec::with_capacity(limit);
        for index in 0..limit {
            sections.push(self.section(index as u32)?);
        }
        Ok(sections)
    }

    /// Read a raw resource (image, stylesheet, …) from the container.
    pub fn resource(&mut self, href: &str) -> Result<Vec<u8>, DocumentError> {
        self.archive
            .read_path(href)
            .map_err(|_| DocumentError::MissingFile(href.to_string()))
    }
}

fn build_section(
    edition_id: &str,
    spine_index: u32,
    href: &str,
    blocks: &[xhtml::TextBlock],
) -> SectionText {
    let spine_id = spine_item_id(edition_id, href);
    let mut out_blocks = Vec::with_capacity(blocks.len());
    let mut fingerprint_input = String::new();
    for (ordinal, block) in blocks.iter().enumerate() {
        let text_sha = sha256_hex(block.text.as_bytes());
        let id = text_block_id(
            edition_id,
            spine_id.as_str(),
            ordinal as u32,
            &text_sha,
        );
        fingerprint_input.push_str(&block.text);
        fingerprint_input.push('\n');
        out_blocks.push(BlockText {
            block_id: id.into_string(),
            ordinal: ordinal as u32,
            text: block.text.clone(),
        });
    }
    SectionText {
        section_id: href.to_string(),
        spine_index: Some(spine_index),
        doc_fingerprint: Some(format!("sha256-{}", sha256_hex(fingerprint_input.as_bytes()))),
        blocks: out_blocks,
    }
}

/// Read `full-path` from `META-INF/container.xml`.
pub fn container_rootfile(container_xml: &[u8]) -> Result<String, DocumentError> {
    let text = String::from_utf8_lossy(container_xml);
    let mut reader = Reader::from_str(&text);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                if local_name(event.name().as_ref()) == "rootfile" {
                    for attribute in event.attributes().flatten() {
                        if local_name(attribute.key.as_ref()) == "full-path" {
                            return Ok(String::from_utf8_lossy(&attribute.value).to_string());
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(source) => {
                return Err(DocumentError::Xml {
                    context: CONTAINER_PATH.into(),
                    source,
                });
            }
        }
    }
    Err(DocumentError::MissingField(
        "rootfile/@full-path".into(),
        CONTAINER_PATH.into(),
    ))
}

fn parse_package(opf: &str, opf_path: &str) -> Result<EpubPackage, DocumentError> {
    let base_dir = dirname(opf_path);
    let mut reader = Reader::from_str(opf);
    reader.config_mut().trim_text(true);

    let mut metadata = EpubMetadata::default();
    let mut manifest: Vec<ManifestItem> = Vec::new();
    let mut spine_refs: Vec<(String, bool, Vec<String>)> = Vec::new();
    let mut current_metadata: Option<String> = None;
    let mut text_buffer = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "item" {
                    if let Some(item) = manifest_item(&event, &base_dir) {
                        manifest.push(item);
                    }
                } else if name == "itemref" {
                    if let Some(reference) = spine_reference(&event) {
                        spine_refs.push(reference);
                    }
                } else if !matches!(name.as_str(), "metadata" | "manifest" | "spine" | "package")
                {
                    current_metadata = Some(name);
                    text_buffer.clear();
                }
            }
            Ok(Event::Empty(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "item" {
                    if let Some(item) = manifest_item(&event, &base_dir) {
                        manifest.push(item);
                    }
                } else if name == "itemref"
                    && let Some(reference) = spine_reference(&event)
                {
                    spine_refs.push(reference);
                }
            }
            Ok(Event::Text(event)) => {
                if current_metadata.is_some() {
                    let decoded = event.decode().map_err(|error| DocumentError::Xml {
                        context: opf_path.to_string(),
                        source: quick_xml::Error::from(error),
                    })?;
                    text_buffer.push_str(&decoded);
                }
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref());
                if current_metadata.as_deref() == Some(name.as_str()) {
                    record_metadata(&mut metadata, &name, &text_buffer);
                    current_metadata = None;
                    text_buffer.clear();
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(source) => {
                return Err(DocumentError::Xml {
                    context: opf_path.to_string(),
                    source,
                });
            }
        }
    }

    let mut spine = Vec::with_capacity(spine_refs.len());
    for (ordinal, (idref, linear, properties)) in spine_refs.into_iter().enumerate() {
        let Some(item) = manifest.iter().find(|item| item.id == idref) else {
            // A dangling itemref is a broken package document; report it rather
            // than silently skipping a chapter.
            return Err(DocumentError::MissingField(
                format!("manifest item {idref:?}"),
                opf_path.to_string(),
            ));
        };
        spine.push(SpineItem {
            idref,
            href: item.href.clone(),
            media_type: item.media_type.clone(),
            linear,
            properties,
            ordinal: ordinal as u32,
        });
    }

    Ok(EpubPackage {
        opf_path: opf_path.to_string(),
        metadata,
        manifest,
        spine,
        nav: Vec::new(),
        nav_source: None,
    })
}

fn manifest_item(event: &quick_xml::events::BytesStart<'_>, base_dir: &str) -> Option<ManifestItem> {
    let id = attribute(event, "id")?;
    let raw_href = attribute(event, "href")?;
    let media_type = attribute(event, "media-type").unwrap_or_default();
    let properties = attribute(event, "properties")
        .map(|value| split_tokens(&value))
        .unwrap_or_default();
    Some(ManifestItem {
        id,
        href: resolve_href(base_dir, &raw_href),
        media_type,
        properties,
    })
}

/// Read `<itemref idref="…" linear="…" properties="…"/>`.
fn spine_reference(event: &quick_xml::events::BytesStart<'_>) -> Option<(String, bool, Vec<String>)> {
    let idref = attribute(event, "idref")?;
    let linear = attribute(event, "linear")
        .map(|value| value != "no")
        .unwrap_or(true);
    let properties = attribute(event, "properties")
        .map(|value| split_tokens(&value))
        .unwrap_or_default();
    Some((idref, linear, properties))
}

fn record_metadata(metadata: &mut EpubMetadata, name: &str, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    match name {
        "title" => {
            if metadata.title.is_none() {
                metadata.title = Some(value.to_string());
            }
        }
        "creator" => metadata.authors.push(value.to_string()),
        "language" => {
            if metadata.language.is_none() {
                metadata.language = Some(value.to_string());
            }
        }
        "identifier" => {
            if metadata.identifier.is_none() {
                metadata.identifier = Some(value.to_string());
            }
        }
        "publisher" => {
            if metadata.publisher.is_none() {
                metadata.publisher = Some(value.to_string());
            }
        }
        other => metadata
            .extra
            .entry(other.to_string())
            .or_default()
            .push(value.to_string()),
    }
}

fn load_navigation<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    package: &EpubPackage,
) -> Result<(Vec<NavNode>, Option<NavSource>), DocumentError> {
    if let Some(item) = package
        .manifest
        .iter()
        .find(|item| item.properties.iter().any(|value| value == "nav"))
    {
        let bytes = archive
            .read_path(&item.href)
            .map_err(|_| DocumentError::MissingFile(item.href.clone()))?;
        let text = String::from_utf8_lossy(&bytes).to_string();
        let base_dir = dirname(&item.href);
        let nav = parse_nav_document(&text, &base_dir)?;
        return Ok((nav, Some(NavSource::Nav)));
    }
    if let Some(item) = package
        .manifest
        .iter()
        .find(|item| item.media_type == "application/x-dtbncx+xml")
    {
        let bytes = archive
            .read_path(&item.href)
            .map_err(|_| DocumentError::MissingFile(item.href.clone()))?;
        let text = String::from_utf8_lossy(&bytes).to_string();
        let base_dir = dirname(&item.href);
        let nav = parse_ncx_document(&text, &base_dir)?;
        return Ok((nav, Some(NavSource::Ncx)));
    }
    Ok((Vec::new(), None))
}

/// A navigation entry before it is nested.
struct FlatNav {
    depth: usize,
    node: NavNode,
}

/// Nest a flat list of entries by depth, preserving order.
fn build_tree(flat: Vec<FlatNav>) -> Vec<NavNode> {
    let mut children_of: Vec<Vec<usize>> = vec![Vec::new(); flat.len()];
    let mut roots: Vec<usize> = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for (index, entry) in flat.iter().enumerate() {
        while let Some(&(depth, _)) = stack.last()
            && depth >= entry.depth
        {
            stack.pop();
        }
        match stack.last() {
            Some(&(_, parent)) => children_of[parent].push(index),
            None => roots.push(index),
        }
        stack.push((entry.depth, index));
    }

    fn materialize(index: usize, flat: &[FlatNav], children_of: &[Vec<usize>]) -> NavNode {
        let mut node = flat[index].node.clone();
        node.children = children_of[index]
            .iter()
            .map(|child| materialize(*child, flat, children_of))
            .collect();
        node
    }

    roots
        .iter()
        .map(|root| materialize(*root, &flat, &children_of))
        .collect()
}

/// Parse an EPUB 3 navigation document.
pub fn parse_nav_document(xhtml: &str, base_dir: &str) -> Result<Vec<NavNode>, DocumentError> {
    let mut reader = Reader::from_str(xhtml);
    reader.config_mut().trim_text(true);
    let mut flat: Vec<FlatNav> = Vec::new();
    let mut ol_depth = 0usize;
    let mut in_anchor = false;
    let mut anchor_href = String::new();
    let mut anchor_label = String::new();
    let mut in_toc = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref());
                match name.as_str() {
                    "nav" => {
                        let kind = attribute(&event, "type").unwrap_or_default();
                        in_toc = kind.split_whitespace().any(|value| value == "toc") || kind.is_empty();
                    }
                    "ol" if in_toc => ol_depth += 1,
                    "a" if in_toc => {
                        in_anchor = true;
                        anchor_href = attribute(&event, "href").unwrap_or_default();
                        anchor_label.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(event)) => {
                if in_anchor {
                    let decoded = event.decode().map_err(|error| DocumentError::Xml {
                        context: "nav document".into(),
                        source: quick_xml::Error::from(error),
                    })?;
                    anchor_label.push_str(&decoded);
                }
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref());
                match name.as_str() {
                    "nav" => in_toc = false,
                    "ol" if in_toc => ol_depth = ol_depth.saturating_sub(1),
                    "a" if in_anchor => {
                        in_anchor = false;
                        let (href, fragment) = split_fragment(&resolve_href(base_dir, &anchor_href));
                        flat.push(FlatNav {
                            depth: ol_depth.max(1),
                            node: NavNode {
                                label: anchor_label.trim().to_string(),
                                href,
                                fragment,
                                children: Vec::new(),
                            },
                        });
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(source) => {
                return Err(DocumentError::Xml {
                    context: "nav document".into(),
                    source,
                });
            }
        }
    }
    Ok(build_tree(flat))
}

/// Parse an EPUB 2 NCX document.
pub fn parse_ncx_document(ncx: &str, base_dir: &str) -> Result<Vec<NavNode>, DocumentError> {
    let mut reader = Reader::from_str(ncx);
    reader.config_mut().trim_text(true);
    let mut flat: Vec<FlatNav> = Vec::new();
    let mut depth = 0usize;
    let mut label_target = false;
    let mut label = String::new();
    let mut href = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref());
                match name.as_str() {
                    "navpoint" => {
                        depth += 1;
                        label.clear();
                        href.clear();
                    }
                    "text" => label_target = true,
                    "content" => {
                        href = attribute(&event, "src").unwrap_or_default();
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(event)) => {
                if local_name(event.name().as_ref()) == "content" {
                    href = attribute(&event, "src").unwrap_or_default();
                }
            }
            Ok(Event::Text(event)) => {
                if label_target {
                    let decoded = event.decode().map_err(|error| DocumentError::Xml {
                        context: "ncx document".into(),
                        source: quick_xml::Error::from(error),
                    })?;
                    label.push_str(&decoded);
                }
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref());
                match name.as_str() {
                    "text" => label_target = false,
                    "navpoint" => {
                        let path = resolve_href(base_dir, &href);
                        let (href, fragment) = split_fragment(&path);
                        flat.push(FlatNav {
                            depth: depth.max(1),
                            node: NavNode {
                                label: label.trim().to_string(),
                                href,
                                fragment,
                                children: Vec::new(),
                            },
                        });
                        depth = depth.saturating_sub(1);
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(source) => {
                return Err(DocumentError::Xml {
                    context: "ncx document".into(),
                    source,
                });
            }
        }
    }
    Ok(build_tree(flat))
}

fn split_fragment(path: &str) -> (String, Option<String>) {
    match path.split_once('#') {
        Some((href, fragment)) if !fragment.is_empty() => {
            (href.to_string(), Some(fragment.to_string()))
        }
        Some((href, _)) => (href.to_string(), None),
        None => (path.to_string(), None),
    }
}

fn attribute(event: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<String> {
    for attribute in event.attributes().flatten() {
        if local_name(attribute.key.as_ref()) == name {
            return Some(String::from_utf8_lossy(&attribute.value).to_string());
        }
    }
    None
}

fn split_tokens(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>()
}

fn local_name(raw: &[u8]) -> String {
    let name = String::from_utf8_lossy(raw);
    let name = name.rsplit(':').next().unwrap_or(&name);
    name.to_ascii_lowercase()
}

fn dirname(path: &str) -> String {
    match path.rfind('/') {
        Some(index) => path[..index].to_string(),
        None => String::new(),
    }
}

/// Resolve an href relative to a base directory, decoding percent escapes and
/// normalizing `.`/`..` segments.
pub fn resolve_href(base_dir: &str, href: &str) -> String {
    let decoded = percent_decode(href);
    let combined = if base_dir.is_empty() || decoded.starts_with('/') {
        decoded.trim_start_matches('/').to_string()
    } else {
        format!("{base_dir}/{decoded}")
    };
    let mut segments: Vec<&str> = Vec::new();
    for segment in combined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &value[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}
