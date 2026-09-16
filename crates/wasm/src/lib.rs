//! `reader-wasm` — wasm-bindgen facade that exposes the pure-Rust document
//! parser and annotation relocating pipeline to the web shell.
//!
//! The handle table is `thread_local!` + `RefCell<HashMap>` because WASM is
//! single-threaded; no Mutex needed. All heavy data crossing the JS ↔ Rust
//! boundary goes through serde JSON — the core types (`SectionText`,
//! `BlockText`, `AnnotationAnchor`, `Relocation`) already derive
//! `Serialize`/`Deserialize`.

use std::collections::HashMap;
use std::io::Cursor;
use std::rc::Rc;
use std::cell::RefCell;

use wasm_bindgen::prelude::*;

use reader_annotation::relocate::{relocate, TextIndex};
use reader_annotation::AnchorState;
use reader_document::epub::Epub;
use reader_document::txt::TxtDocument;
use reader_annotation::types::AnnotationAnchor;

#[allow(clippy::large_enum_variant)]
enum HandleInner {
    Epub(Epub<Cursor<Vec<u8>>>),
    Txt { doc: TxtDocument, edition_id: String },
}

thread_local! {
    static HANDLES: RefCell<HashMap<u32, HandleInner>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: Rc<RefCell<u32>> = Rc::new(RefCell::new(1));
}

fn alloc_handle() -> u32 {
    NEXT_HANDLE.with(|n| {
        let mut next = n.borrow_mut();
        let h = *next;
        *next = next.checked_add(1).expect("handle overflow");
        h
    })
}

fn make_edition_id() -> String {
    reader_model::id::new_entity_id().into_string()
}

fn with_handle<F, T>(handle: u32, f: F) -> Result<T, String>
where
    F: FnOnce(&mut HandleInner) -> Result<T, String>,
{
    HANDLES.with(|map| {
        let mut map = map.borrow_mut();
        let inner = map.get_mut(&handle).ok_or_else(|| format!("invalid handle {handle}"))?;
        f(inner)
    })
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("json encode: {e}"))
}

fn from_json<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, String> {
    serde_json::from_str(s).map_err(|e| format!("json decode: {e}"))
}

// ---------------------------------------------------------------------------
// EPUB
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub fn epub_open(data: &[u8]) -> Result<u32, String> {
    let reader = Cursor::new(data.to_vec());
    let edition_id = make_edition_id();
    let epub = Epub::open_reader(reader, edition_id).map_err(|e| format!("epub open: {e}"))?;
    let handle = alloc_handle();
    HANDLES.with(|map| map.borrow_mut().insert(handle, HandleInner::Epub(epub)));
    Ok(handle)
}

#[wasm_bindgen]
pub fn epub_close(handle: u32) {
    HANDLES.with(|map| map.borrow_mut().remove(&handle));
}

#[wasm_bindgen]
pub fn epub_index_all(handle: u32, max_sections: usize) -> Result<String, String> {
    with_handle(handle, |inner| match inner {
        HandleInner::Epub(epub) => {
            let sections = epub
                .index_all(max_sections)
                .map_err(|e| format!("epub index: {e}"))?;
            to_json(&sections)
        }
        _ => Err("handle is not an epub".into()),
    })
}

#[wasm_bindgen]
pub fn epub_section(handle: u32, spine_index: u32) -> Result<String, String> {
    with_handle(handle, |inner| match inner {
        HandleInner::Epub(epub) => {
            let section = epub
                .section(spine_index)
                .map_err(|e| format!("epub section: {e}"))?;
            to_json(&section)
        }
        _ => Err("handle is not an epub".into()),
    })
}

#[wasm_bindgen]
pub fn epub_edition_id(handle: u32) -> Result<String, String> {
    with_handle(handle, |inner| match inner {
        HandleInner::Epub(epub) => Ok(epub.edition_id().to_string()),
        _ => Err("handle is not an epub".into()),
    })
}

#[wasm_bindgen]
pub fn epub_spine_count(handle: u32) -> Result<usize, String> {
    with_handle(handle, |inner| match inner {
        HandleInner::Epub(epub) => Ok(epub.package().spine.len()),
        _ => Err("handle is not an epub".into()),
    })
}

// ---------------------------------------------------------------------------
// TXT
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub fn txt_open(data: &[u8], source_name: Option<String>) -> Result<u32, String> {
    let txt = TxtDocument::open_bytes(data.to_vec(), source_name.as_deref())
        .map_err(|e| format!("txt open: {e}"))?;
    let handle = alloc_handle();
    let edition_id = make_edition_id();
    HANDLES.with(|map| map.borrow_mut().insert(handle, HandleInner::Txt { doc: txt, edition_id }));
    Ok(handle)
}

#[wasm_bindgen]
pub fn txt_close(handle: u32) {
    HANDLES.with(|map| map.borrow_mut().remove(&handle));
}

#[wasm_bindgen]
pub fn txt_chapter_count(handle: u32) -> Result<usize, String> {
    with_handle(handle, |inner| match inner {
        HandleInner::Txt { doc, .. } => Ok(doc.chapters().len()),
        _ => Err("handle is not a txt document".into()),
    })
}

#[wasm_bindgen]
pub fn txt_chapter_blocks(handle: u32, ordinal: u32) -> Result<String, String> {
    with_handle(handle, |inner| match inner {
        HandleInner::Txt { doc, edition_id } => {
            let blocks = doc
                .chapter_blocks(edition_id, ordinal)
                .map_err(|e| format!("txt blocks: {e}"))?;
            to_json(&blocks)
        }
        _ => Err("handle is not a txt document".into()),
    })
}

// ---------------------------------------------------------------------------
// Annotation relocation
// ---------------------------------------------------------------------------

/// Run the 7-step annotation relocation pipeline.
///
/// `anchor_json`   — serialised `AnnotationAnchor`
/// `index_json`    — serialised array of `SectionText` (the current book index)
/// Returns serialised `Relocation` with resolved position, confidence and warnings.
#[wasm_bindgen]
pub fn anchor_relocate(anchor_json: &str, index_json: &str) -> Result<String, String> {
    let anchor: AnnotationAnchor = from_json(anchor_json)?;
    let sections: Vec<reader_annotation::relocate::SectionText> = from_json(index_json)?;
    let index = TextIndex::new(sections);
    let result = relocate(&anchor, &index);
    to_json(&result)
}

/// Map a raw confidence score (0..1) to the anchor lifecycle state
/// (`orphaned`, `uncertain`, `fragile`, `stable`).
#[wasm_bindgen]
pub fn anchor_state(confidence: f64) -> String {
    match reader_annotation::relocate::state_for_confidence(confidence) {
        AnchorState::Anchored => "anchored",
        AnchorState::NeedsReview => "needs_review",
        AnchorState::Fuzzy => "fuzzy",
        AnchorState::Orphaned => "orphaned",
    }
    .into()
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub fn handle_count() -> usize {
    HANDLES.with(|map| map.borrow().len())
}

#[wasm_bindgen]
pub fn clear_all_handles() {
    HANDLES.with(|map| map.borrow_mut().clear());
}
