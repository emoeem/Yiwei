//! Cross-language protocol contract.
//!
//! The Rust crates and the TypeScript client must agree on the exact JSON
//! shapes that cross the wire and the database (report §21.3, §22.7). Instead of
//! documenting that by hand, this crate owns a set of canonical fixtures under
//! `packages/protocol/fixtures/`:
//!
//! * Rust tests assert every fixture deserializes into the real model type and
//!   re-serializes to exactly the same JSON, so the fixtures cannot drift,
//! * the TypeScript package parses the same files, so a shape change breaks the
//!   build on both sides.
//!
//! `regenerate` rewrites the fixtures from the Rust examples; it is ignored by
//! default because it modifies files in the repository.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reader_annotation::{
    AnchorOrigin, AnchorScheme, AnchorState, AnnotationAnchor, BlockText, SectionText,
};
use reader_model::{Locator, Rect, TextQuote};
use reader_sync::{
    DeviceId, EntityKind, FieldEnvelope, Hlc, Op, OpKind, ProgressState, Replica,
};
use serde_json::{Value, json};

/// Name of every fixture this crate owns.
pub const FIXTURES: &[&str] = &[
    "locator.json",
    "anchor.json",
    "section.json",
    "replica.json",
    "op.json",
    "progress.json",
];

/// Path of a fixture file.
pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

/// The engine name recorded with a rendered book.
pub fn engine_name() -> &'static str {
    "foliate-js@78914ae"
}

fn device(name: &str) -> DeviceId {
    DeviceId::parse(name).expect("valid device id")
}

fn hlc(physical_ms: u64, counter: u32, dev: &str) -> Hlc {
    Hlc::new(physical_ms, counter, device(dev))
}

/// A locator as stored in `reading_state` and inside an annotation anchor.
pub fn locator_example() -> Locator {
    let mut locator = Locator {
        book_id: "01932b39-0000-7000-8000-0000000000aa".into(),
        edition_id: Some("01932b39-0000-7000-8000-0000000000bb".into()),
        section_id: Some("OEBPS/text/chapter1.xhtml".into()),
        cfi: Some("epubcfi(/6/4[chap01ref]!/4[body01]/10[para05]/1:58)".into()),
        xpointer: None,
        block_id: Some("block-5f3a1c9d2b4e6f8a0c1d2e3f4a5b6c7d".into()),
        start_offset: Some(58),
        end_offset: Some(72),
        text_quote: Some(TextQuote::new("brave sailor", "the ", " set sail")),
        doc_fingerprint: Some("sha256-14c1f0f2b6a8d4e7".into()),
        progression: Some(0.6405),
        page: None,
        rects: Some(vec![Rect {
            x: 0.1,
            y: 0.2,
            width: 0.3,
            height: 0.05,
        }]),
        updated_at: Some("2026-09-15T10:00:00Z".into()),
        extra: BTreeMap::new(),
    };
    // Forward compatibility: a field written by a newer build must survive.
    locator
        .extra
        .insert("future_field".into(), json!({ "nested": [1, 2, 3] }));
    locator
}

/// An annotation anchor as stored in `annotation_anchor`.
pub fn anchor_example() -> AnnotationAnchor {
    AnnotationAnchor {
        scheme: AnchorScheme::EpubCfi,
        primary_locator: locator_example(),
        doc_id: None,
        section_id: Some("OEBPS/text/chapter1.xhtml".into()),
        block_id: Some("block-5f3a1c9d2b4e6f8a0c1d2e3f4a5b6c7d".into()),
        quote: Some(TextQuote::new("brave sailor", "the ", " set sail")),
        start_offset: Some(58),
        end_offset: Some(70),
        doc_fingerprint: Some("sha256-14c1f0f2b6a8d4e7".into()),
        rects: None,
        page: None,
        confidence: 0.97,
        state: AnchorState::Anchored,
        origin: AnchorOrigin::User,
        updated_at: Some("2026-09-15T10:00:00Z".into()),
    }
}

/// A derived section with deterministic block IDs.
pub fn section_example() -> SectionText {
    SectionText {
        section_id: "OEBPS/text/chapter1.xhtml".into(),
        spine_index: Some(1),
        doc_fingerprint: Some("sha256-14c1f0f2b6a8d4e7".into()),
        blocks: vec![
            BlockText {
                block_id: "block-5f3a1c9d2b4e6f8a0c1d2e3f4a5b6c7d".into(),
                ordinal: 0,
                text: "The brave sailor set sail.".into(),
            },
            BlockText {
                block_id: "block-0a1b2c3d4e5f60718293a4b5c6d7e8f9".into(),
                ordinal: 1,
                text: "The storm arrived at dawn.".into(),
            },
        ],
    }
}

/// A replica row with a tombstone and a reincarnation token.
pub fn replica_example() -> Replica {
    let mut replica = Replica::new(hlc(1_762_000_000_000, 0, "01932b39-0000-7000-8000-0000000000a1"));
    replica.set_field(
        "note",
        json!("first thought"),
        hlc(1_762_000_000_000, 0, "01932b39-0000-7000-8000-0000000000a1"),
    );
    replica.set_field(
        "color",
        json!("yellow"),
        hlc(1_762_000_000_500, 3, "01932b39-0000-7000-8000-0000000000a2"),
    );
    replica.tombstone(hlc(1_762_000_100_000, 0, "01932b39-0000-7000-8000-0000000000a2"));
    replica
        .restore(
            "revive-8c1f",
            hlc(1_762_000_200_000, 0, "01932b39-0000-7000-8000-0000000000a2"),
        )
        .expect("restore");
    replica
}

/// A sync operation in its wire form.
pub fn op_example() -> Op {
    let mut fields = BTreeMap::new();
    fields.insert(
        "note".to_string(),
        FieldEnvelope::new(
            json!("first thought"),
            hlc(1_762_000_000_000, 0, "01932b39-0000-7000-8000-0000000000a1"),
        ),
    );
    let mut op = Op {
        op_id: "01932b39-0000-7000-8000-0000000000c1".into(),
        entity: EntityKind::Annotation,
        entity_id: "01932b39-0000-7000-8000-0000000000d1".into(),
        kind: OpKind::Upsert,
        base_revision: Some(12),
        fields,
        hlc: hlc(1_762_000_000_000, 0, "01932b39-0000-7000-8000-0000000000a1"),
        device_id: device("01932b39-0000-7000-8000-0000000000a1"),
        payload_hash: None,
    };
    op.payload_hash = Some(op.computed_payload_hash());
    op
}

/// Reading progress as merged by the sync engine.
pub fn progress_example() -> ProgressState {
    ProgressState {
        current: locator_example(),
        current_hlc: hlc(1_762_000_300_000, 1, "01932b39-0000-7000-8000-0000000000a1"),
        furthest: locator_example(),
        furthest_hlc: hlc(1_762_000_400_000, 0, "01932b39-0000-7000-8000-0000000000a2"),
    }
}

/// Fixture name → canonical JSON value.
pub fn examples() -> Vec<(&'static str, Value)> {
    vec![
        ("locator.json", to_value(locator_example())),
        ("anchor.json", to_value(anchor_example())),
        ("section.json", to_value(section_example())),
        ("replica.json", to_value(replica_example())),
        ("op.json", to_value(op_example())),
        ("progress.json", to_value(progress_example())),
    ]
}

/// Parse a fixture into its model type, proving the JSON is valid for us.
pub fn parse_fixture(name: &str, value: &Value) -> Result<(), serde_json::Error> {
    match name {
        "locator.json" => serde_json::from_value::<Locator>(value.clone()).map(|_| ()),
        "anchor.json" => serde_json::from_value::<AnnotationAnchor>(value.clone()).map(|_| ()),
        "section.json" => serde_json::from_value::<SectionText>(value.clone()).map(|_| ()),
        "replica.json" => serde_json::from_value::<Replica>(value.clone()).map(|_| ()),
        "op.json" => serde_json::from_value::<Op>(value.clone()).map(|_| ()),
        "progress.json" => serde_json::from_value::<ProgressState>(value.clone()).map(|_| ()),
        other => panic!("unknown fixture {other}"),
    }
}

fn to_value(value: impl serde::Serialize) -> Value {
    serde_json::to_value(value).expect("fixture serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_fixtures_match_the_rust_model() {
        for (name, example) in examples() {
            let path = fixture_path(name);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
            let stored: Value = serde_json::from_str(&text)
                .unwrap_or_else(|error| panic!("{name} is not valid JSON: {error}"));
            assert_eq!(
                stored, example,
                "{name} drifted from the Rust model; run `cargo test -p reader-protocol -- --ignored regenerate`"
            );
            parse_fixture(name, &stored).unwrap_or_else(|error| {
                panic!("{name} does not deserialize into the Rust model: {error}")
            });
        }
    }

    #[test]
    fn fixture_list_is_complete() {
        let names: Vec<&str> = examples().into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, FIXTURES);
    }

    #[test]
    fn op_payload_hash_is_present_in_the_fixture() {
        let value = to_value(op_example());
        assert!(
            value.get("payloadHash").is_some(),
            "the fixture must pin the payload hash so the client can verify integrity"
        );
    }

    /// Rewrite the fixtures from the Rust examples.
    ///
    /// Ignored by default because it writes into the repository; run it after
    /// intentionally changing a protocol shape:
    /// `cargo test -p reader-protocol -- --ignored regenerate`.
    #[test]
    #[ignore]
    fn regenerate() {
        for (name, value) in examples() {
            let path = fixture_path(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create fixture directory");
            }
            let mut text = serde_json::to_string_pretty(&value).expect("serialize");
            text.push('\n');
            std::fs::write(&path, text).expect("write fixture");
            println!("wrote {}", path.display());
        }
    }
}
