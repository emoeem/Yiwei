//! Tests for the schema, migrations and repositories.

use std::collections::BTreeMap;

use reader_model::{Locator, TextQuote};
use reader_sync::{DeviceId, EntityKind, FieldEnvelope, Hlc, Op, OpKind};

use crate::db::Database;
use crate::error::StorageError;
use crate::migrations::{CURRENT_SCHEMA_VERSION, apply};
use crate::repos::{
    AnchorRow, AnnotationRow, Annotations, NoteRevisionRow, Outbox, PublicationRow, Publications,
    ReadingStateRow, ReadingStates, Search, SettingRow, Settings,
};

fn device(name: &str) -> DeviceId {
    DeviceId::parse(name).expect("device")
}

fn sample_publication(id: &str) -> PublicationRow {
    PublicationRow {
        id: id.into(),
        uuid: Some("urn:uuid:1234".into()),
        title: "Test Book".into(),
        authors: vec!["Ada Lovelace".into(), "Alan Turing".into()],
        language: Some("en".into()),
        metadata: serde_json::json!({
            "publisher": "Test Press",
            "future_field": { "kept": true }
        }),
        created_at: 1_700_000_000,
        updated_at: 1_700_000_100,
        deleted_at: None,
        rev: 1,
        device_id: Some("dev-a".into()),
    }
}

fn sample_annotation(id: &str, publication_id: &str) -> AnnotationRow {
    AnnotationRow {
        id: id.into(),
        publication_id: publication_id.into(),
        edition_id: Some("edition-1".into()),
        annotation_type: "highlight".into(),
        color: Some("yellow".into()),
        style: None,
        note: Some("first thought".into()),
        selected_text: Some("brave sailor".into()),
        tags: vec!["sail".into()],
        global: false,
        created_at: 1_700_000_200,
        updated_at: 1_700_000_200,
        deleted_at: None,
        rev: 1,
        device_id: Some("dev-a".into()),
    }
}

#[test]
fn migrates_to_the_current_version_and_creates_every_table() {
    let database = Database::open_in_memory().expect("open");
    assert_eq!(database.schema_version().expect("version"), CURRENT_SCHEMA_VERSION);

    let expected = [
        "publication",
        "book_file",
        "edition",
        "spine_item",
        "nav_node",
        "text_block",
        "reading_state",
        "annotation",
        "annotation_anchor",
        "note_revision",
        "source",
        "source_rule",
        "plugin",
        "job",
        "reading_session",
        "stat_daily",
        "glossary_term",
        "ai_run",
        "setting",
        "sync_outbox",
        "sync_state",
        "schema_migration",
    ];
    let mut statement = database
        .connection()
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .expect("prepare");
    let names: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query")
        .map(|row| row.expect("row"))
        .collect();
    for table in expected {
        assert!(names.contains(&table.to_string()), "missing table {table}");
    }
}

#[test]
fn migration_is_idempotent_and_refuses_a_newer_database() {
    let mut connection = rusqlite::Connection::open_in_memory().expect("open");
    assert_eq!(apply(&mut connection).expect("first"), CURRENT_SCHEMA_VERSION);
    assert_eq!(apply(&mut connection).expect("second"), CURRENT_SCHEMA_VERSION);
    let rows: i64 = connection
        .query_row("SELECT COUNT(*) FROM schema_migration", [], |row| row.get(0))
        .expect("count");
    assert_eq!(rows, 1, "migrations are not recorded twice");

    connection
        .execute(
            "INSERT INTO schema_migration(version, name, applied_at) VALUES (99, 'from the future', 0)",
            [],
        )
        .expect("insert");
    assert!(matches!(
        apply(&mut connection).unwrap_err(),
        StorageError::SchemaTooNew { found: 99, .. }
    ));
}

#[test]
fn publications_round_trip_with_unknown_metadata() {
    let database = Database::open_in_memory().expect("open");
    let publications = Publications::new(database.connection());

    let row = sample_publication("pub-1");
    publications.save(&row).expect("save");
    let mut second = sample_publication("pub-2");
    second.uuid = Some("urn:uuid:5678".into());
    publications.save(&second).expect("save");

    let loaded = publications.get("pub-1").expect("get").expect("present");
    assert_eq!(loaded, row, "unknown metadata fields survive a round trip");
    assert_eq!(publications.count().expect("count"), 2);

    let mut updated = row.clone();
    updated.title = "Renamed".into();
    updated.updated_at = 1_700_000_500;
    updated.rev = 2;
    publications.save(&updated).expect("update");
    let listed = publications.list(false).expect("list");
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].title, "Renamed", "newest first");

    assert_eq!(publications.soft_delete("pub-1", 1_700_000_600).expect("delete"), 1);
    assert_eq!(publications.list(false).expect("list").len(), 1);
    assert_eq!(publications.list(true).expect("list all").len(), 2);
    let deleted = publications.get("pub-1").expect("get").expect("present");
    assert_eq!(deleted.deleted_at, Some(1_700_000_600));
    assert_eq!(deleted.rev, 3, "the tombstone bumps the revision");
}

#[test]
fn annotations_anchors_and_note_revisions() {
    let database = Database::open_in_memory().expect("open");
    Publications::new(database.connection())
        .save(&sample_publication("pub-1"))
        .expect("publication");
    let annotations = Annotations::new(database.connection());

    let annotation = sample_annotation("ann-1", "pub-1");
    annotations.save(&annotation).expect("save");
    annotations
        .save(&sample_annotation("ann-2", "pub-1"))
        .expect("save");

    let locator = Locator {
        book_id: "pub-1".into(),
        edition_id: Some("edition-1".into()),
        section_id: Some("OEBPS/text/ch1.xhtml".into()),
        cfi: Some("epubcfi(/6/2!/4/2:6)".into()),
        block_id: Some("block-1".into()),
        start_offset: Some(6),
        end_offset: Some(11),
        text_quote: Some(TextQuote::new("brave", "Hello ", " sailor")),
        progression: Some(0.05),
        ..Locator::default()
    };
    let anchor = AnchorRow {
        annotation_id: "ann-1".into(),
        scheme: "epub_cfi".into(),
        primary_locator: locator.clone(),
        doc_id: None,
        section_id: Some("OEBPS/text/ch1.xhtml".into()),
        block_id: Some("block-1".into()),
        exact_text: Some("brave".into()),
        prefix_text: Some("Hello ".into()),
        suffix_text: Some(" sailor".into()),
        start_offset: Some(6),
        end_offset: Some(11),
        doc_fingerprint: Some("sha256-abc".into()),
        rects: None,
        page: None,
        confidence: 1.0,
        anchor_state: "anchored".into(),
        updated_at: 1_700_000_200,
    };
    annotations.save_anchor(&anchor).expect("anchor");

    let loaded_anchor = annotations
        .anchor("ann-1")
        .expect("anchor query")
        .expect("present");
    assert_eq!(loaded_anchor, anchor);
    assert_eq!(loaded_anchor.primary_locator, locator);

    for (index, content) in ["first thought", "second thought"].iter().enumerate() {
        annotations
            .add_note_revision(&NoteRevisionRow {
                id: format!("rev-{index}"),
                annotation_id: "ann-1".into(),
                base_rev: Some(index as i64),
                content: (*content).into(),
                created_at: 1_700_000_200 + index as i64,
                device_id: Some("dev-a".into()),
            })
            .expect("revision");
    }
    let revisions = annotations.note_revisions("ann-1").expect("revisions");
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[1].content, "second thought");

    assert_eq!(
        annotations.list_for_publication("pub-1", false).expect("list").len(),
        2
    );
    annotations.soft_delete("ann-1", 1_700_000_700).expect("delete");
    assert_eq!(
        annotations.list_for_publication("pub-1", false).expect("list").len(),
        1
    );
    assert!(annotations.anchor("ann-1").expect("anchor").is_some(), "anchors stay for review");
}

#[test]
fn reading_state_and_settings_round_trip() {
    let database = Database::open_in_memory().expect("open");
    let states = ReadingStates::new(database.connection());

    let current = Locator {
        book_id: "pub-1".into(),
        edition_id: Some("edition-1".into()),
        section_id: Some("OEBPS/text/ch2.xhtml".into()),
        cfi: Some("epubcfi(/6/4!/4/2:10)".into()),
        block_id: Some("block-9".into()),
        start_offset: Some(10),
        end_offset: Some(10),
        progression: Some(0.42),
        ..Locator::default()
    };
    let row = ReadingStateRow {
        publication_id: "pub-1".into(),
        edition_id: "edition-1".into(),
        device_id: "dev-a".into(),
        locator: current,
        progress: 0.42,
        furthest: None,
        updated_at: 1_700_000_300,
        hlc: Some("0000018e7d6ab5c0-00000000-dev-a".into()),
    };
    states.put(&row).expect("put");
    assert_eq!(states.get("pub-1", "edition-1", "dev-a").expect("get"), Some(row.clone()));

    let mut second = row.clone();
    second.device_id = "dev-b".into();
    second.progress = 0.91;
    second.updated_at = 1_700_000_400;
    states.put(&second).expect("put");
    let listed = states.list_for_publication("pub-1").expect("list");
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].device_id, "dev-b", "most recent first");

    let settings = Settings::new(database.connection());
    settings
        .put(&SettingRow {
            key: "reader.theme".into(),
            value: serde_json::json!({ "name": "sepia", "font_scale": 1.2 }),
            hlc: Some("0000018e7d6ab5c0-00000001-dev-a".into()),
            device_id: Some("dev-a".into()),
        })
        .expect("put");
    let stored = settings.get("reader.theme").expect("get").expect("present");
    assert_eq!(stored.value["name"], serde_json::json!("sepia"));
    assert_eq!(settings.all().expect("all").len(), 1);
}

#[test]
fn outbox_queues_operations_and_tracks_failures() {
    let database = Database::open_in_memory().expect("open");
    let outbox = Outbox::new(database.connection());

    let mut fields = BTreeMap::new();
    fields.insert(
        "note".to_string(),
        FieldEnvelope::new(
            serde_json::json!("hello"),
            Hlc::new(1_700_000_000_000, 0, device("dev-a")),
        ),
    );
    let op = Op {
        op_id: "op-1".into(),
        entity: EntityKind::Annotation,
        entity_id: "ann-1".into(),
        kind: OpKind::Upsert,
        base_revision: Some(1),
        fields,
        hlc: Hlc::new(1_700_000_000_000, 0, device("dev-a")),
        device_id: device("dev-a"),
        payload_hash: None,
    };
    outbox.enqueue(&op, 1_700_000_000).expect("enqueue");
    outbox.enqueue(&op, 1_700_000_000).expect("enqueue twice");
    assert_eq!(outbox.count().expect("count"), 1, "opId makes enqueue idempotent");

    let pending = outbox.pending(10).expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].op, op, "the operation survives a JSON round trip");
    assert_eq!(pending[0].attempts, 0);

    outbox.record_failure("op-1", "network unreachable").expect("failure");
    let pending = outbox.pending(10).expect("pending");
    assert_eq!(pending[0].attempts, 1);
    assert_eq!(pending[0].last_error.as_deref(), Some("network unreachable"));

    assert_eq!(outbox.mark_pushed("op-1").expect("pushed"), 1);
    assert_eq!(outbox.count().expect("count"), 0);
}

#[test]
fn full_text_search_reports_its_capability_honestly() {
    let database = Database::open_in_memory().expect("open");
    let capabilities = database.capabilities();
    let search = Search::new(database.connection(), capabilities.fts5);

    let blocks = vec![
        ("block-1".to_string(), "The brave sailor set sail.".to_string()),
        ("block-2".to_string(), "The storm arrived at dawn.".to_string()),
    ];
    if capabilities.fts5 {
        let indexed = search.index_section("edition-1", "spine-1", &blocks).expect("index");
        assert_eq!(indexed, 2);
        let hits = search.query("edition-1", "sailor", 10).expect("query");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].block_id, "block-1");

        let other = search.query("edition-2", "sailor", 10).expect("query");
        assert!(other.is_empty(), "search is scoped to one edition");
    } else {
        assert!(matches!(
            search.index_section("edition-1", "spine-1", &blocks).unwrap_err(),
            StorageError::MissingCapability(_)
        ));
        assert!(matches!(
            search.query("edition-1", "sailor", 10).unwrap_err(),
            StorageError::MissingCapability(_)
        ));
    }
}

// ---------------------------------------------------------------------------
// End-to-end integration: fixture EPUB → document.index_all → edition/spine/
// text_block tables → FTS → query. Route-map step 3 of §27.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod integration {
    use reader_document::zip_writer::ZipWriter;
    use reader_document::Epub;
    use reader_model::{deterministic_id, sha256_hex, spine_item_id};

    use crate::db::Database;
    use crate::repos::{
        EditionRow, Editions, NavNodeRow, NavNodes, PublicationRow, Publications, Search,
        SpineItemRow, SpineItems, TextBlockRow, TextBlocks,
    };

    const CONTAINER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

    const PACKAGE_OPF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Test Book</dc:title>
    <dc:creator>Ada Lovelace</dc:creator>
    <dc:language>en</dc:language>
    <dc:identifier id="bookid">urn:uuid:test</dc:identifier>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="text/ch2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
    <itemref idref="ch2"/>
  </spine>
</package>"#;

    const NAV_XHTML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
  <head><title>Contents</title></head>
  <body>
    <nav epub:type="toc"><ol>
      <li><a href="text/ch1.xhtml">Chapter 1</a></li>
      <li><a href="text/ch2.xhtml">Chapter 2</a></li>
    </ol></nav>
  </body>
</html>"#;

    const CH1_XHTML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body>
  <h1>Chapter 1</h1>
  <p>The brave sailor set sail across the ocean blue.</p>
  <p>A second paragraph about navigation and stars.</p>
</body></html>"#;

    const CH2_XHTML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body>
  <h1>Chapter 2</h1>
  <p>The storm arrived and the brave sailor fought the waves.</p>
</body></html>"#;

    fn write_epub(path: &std::path::Path) {
        let mut w = ZipWriter::new(std::fs::File::create(path).expect("create"));
        w.add_stored("mimetype", b"application/epub+zip").unwrap();
        w.add_deflated("META-INF/container.xml", CONTAINER_XML.as_bytes()).unwrap();
        w.add_deflated("OEBPS/content.opf", PACKAGE_OPF.as_bytes()).unwrap();
        w.add_deflated("OEBPS/nav.xhtml", NAV_XHTML.as_bytes()).unwrap();
        w.add_deflated("OEBPS/text/ch1.xhtml", CH1_XHTML.as_bytes()).unwrap();
        w.add_deflated("OEBPS/text/ch2.xhtml", CH2_XHTML.as_bytes()).unwrap();
        w.finish().unwrap();
    }

    #[test]
    fn import_epub_builds_index_and_searches() {
        let tmp = std::env::temp_dir().join(format!(
            "reader-integration-{}-{}.epub",
            std::process::id(),
            deterministic_id("integration", &[])
        ));
        write_epub(&tmp);

        let edition_id = deterministic_id("edition", &[]).into_string();
        let publication_id = deterministic_id("publication", &[]).into_string();

        // Step 1 — parse and index the EPUB through document crate
        let mut epub = Epub::open(&tmp, edition_id.clone()).expect("open epub");
        let sections = epub.index_all(0).expect("index_all");
        assert_eq!(sections.len(), 2, "ch1 + ch2 spine items");

        // Step 2 — open the local database
        let db = Database::open_in_memory().expect("db");
        let caps = db.capabilities();

        // Step 3 — write publication + edition rows
        let publications = Publications::new(db.connection());
        publications
            .save(&PublicationRow {
                id: publication_id.clone(),
                uuid: Some("urn:uuid:test".into()),
                title: "Test Book".into(),
                authors: vec!["Ada Lovelace".into()],
                language: Some("en".into()),
                metadata: serde_json::json!({"publisher": "Test Press"}),
                created_at: 1_700_000_000,
                updated_at: 1_700_000_000,
                deleted_at: None,
                rev: 1,
                device_id: Some("dev-test".into()),
            })
            .expect("save publication");

        let editions = Editions::new(db.connection());
        editions
            .save(&EditionRow {
                id: edition_id.clone(),
                publication_id: publication_id.clone(),
                kind: Some("epub".into()),
                language: Some("en".into()),
                source_edition_id: None,
                file_id: None,
                title: Some("Test Book".into()),
                metadata: serde_json::json!({"opf_version": "3.0"}),
                created_at: 1_700_000_000,
                updated_at: 1_700_000_000,
            })
            .expect("save edition");

        // Step 4 — write spine items + nav nodes from the parsed package
        let spine = epub.package().spine.clone();
        let nav_tree = epub.package().nav.clone();

        let spine_repo = SpineItems::new(db.connection());
        for (ordinal, item) in spine.iter().enumerate() {
            spine_repo
                .save(&SpineItemRow {
                    id: spine_item_id(&edition_id, &item.href).into_string(),
                    edition_id: edition_id.clone(),
                    ordinal: ordinal as i64,
                    href: item.href.clone(),
                    media_type: Some("application/xhtml+xml".into()),
                    linear: item.linear,
                    properties: serde_json::Value::Null,
                })
                .expect("save spine");
        }

        // Flatten the nested NavNode tree into a linear list with ordinal positions
        fn flatten_nav(
            nodes: &[reader_document::epub::NavNode],
            parent_id: Option<String>,
            out: &mut Vec<(Option<String>, reader_document::epub::NavNode)>,
        ) {
            for node in nodes {
                let node_id = deterministic_id("nav", &[&node.href]).into_string();
                out.push((parent_id.clone(), node.clone()));
                flatten_nav(&node.children, Some(node_id), out);
            }
        }
        let mut flat_nav: Vec<(Option<String>, reader_document::epub::NavNode)> = Vec::new();
        flatten_nav(&nav_tree, None, &mut flat_nav);

        let nav_repo = NavNodes::new(db.connection());
        for (ordinal, (parent_id, node)) in flat_nav.iter().enumerate() {
            nav_repo
                .save(&NavNodeRow {
                    id: deterministic_id("nav", &[&node.href]).into_string(),
                    edition_id: edition_id.clone(),
                    parent_id: parent_id.clone(),
                    ordinal: ordinal as i64,
                    label: node.label.clone(),
                    href: node.href.clone(),
                    fragment: node.fragment.clone(),
                })
                .expect("save nav");
        }

        // Step 5 — write text_block rows AND push into the FTS index
        let blocks_repo = TextBlocks::new(db.connection());
        let search = Search::new(db.connection(), caps.fts5);

        let mut fts_block_count = 0usize;
        for section in &sections {
            let spine_id = spine_item_id(&edition_id, &section.section_id).into_string();
            for block in &section.blocks {
                let text_sha = sha256_hex(block.text.as_bytes());
                blocks_repo
                    .save(&TextBlockRow {
                        id: block.block_id.clone(),
                        edition_id: edition_id.clone(),
                        spine_item_id: spine_id.clone(),
                        ordinal: block.ordinal as i64,
                        block_kind: match block.ordinal {
                            0 => "heading",
                            _ => "paragraph",
                        }
                        .into(),
                        text: block.text.clone(),
                        text_sha256: Some(text_sha),
                        doc_fingerprint: section.doc_fingerprint.clone(),
                    })
                    .expect("save text block");
            }

            // Feed this section's blocks into FTS
            let pairs: Vec<(String, String)> = section
                .blocks
                .iter()
                .map(|b| (b.block_id.clone(), b.text.clone()))
                .collect();
            match search.index_section(&edition_id, &spine_id, &pairs) {
                Ok(n) => fts_block_count += n,
                Err(crate::error::StorageError::MissingCapability(_)) => {
                    assert!(!caps.fts5, "fts was promised to be available")
                }
                Err(e) => panic!("unexpected FTS error: {e}"),
            }
        }

        // Sanity: we indexed every block from both sections
        assert_eq!(fts_block_count, 5, "2 heading + 3 paragraph blocks total");

        // Step 6 — verify the full-text search scoped to this edition
        if caps.fts5 {
            let hits = search.query(&edition_id, "sailor", 10).expect("query sailor");
            assert!(!hits.is_empty(), "sailor should match in ch1 AND ch2");
            assert_eq!(hits.len(), 2);
            for hit in &hits {
                assert!(hit.text.contains("sailor"));
            }

            let waves = search.query(&edition_id, "waves", 10).expect("query waves");
            assert_eq!(waves.len(), 1);
            assert!(waves[0].text.contains("waves"));

            let none = search.query(&edition_id, "xzxyx", 10).expect("query nonsense");
            assert!(none.is_empty());

            // Verify the edition-aware text_block rows round-trip
            let all_blocks = blocks_repo.list_for_edition(&edition_id).expect("all blocks");
            assert_eq!(all_blocks.len(), 5);
            let ch1_blocks = blocks_repo
                .list_for_spine_item(
                    spine_item_id(&edition_id, "OEBPS/text/ch1.xhtml").as_str(),
                )
                .expect("ch1 blocks");
            assert_eq!(ch1_blocks.len(), 3, "heading + 2 paragraphs");
        }

        // Verify spine + nav rows round-trip
        let stored_spine = spine_repo
            .list_for_edition(&edition_id)
            .expect("list spine");
        assert_eq!(stored_spine.len(), 2);
        assert_eq!(stored_spine[0].href, "OEBPS/text/ch1.xhtml");

        let stored_nav = nav_repo.list_for_edition(&edition_id).expect("list nav");
        assert_eq!(stored_nav.len(), 2);

        let stored_edition = editions
            .get(&edition_id)
            .expect("get edition")
            .expect("edition exists");
        assert_eq!(stored_edition.publication_id, publication_id);

        // Clean up fixture file
        let _ = std::fs::remove_file(&tmp);
    }
}
