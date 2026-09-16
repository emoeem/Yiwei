//! Tests for the ZIP, EPUB, XHTML and TXT readers.
//!
//! Fixtures are real files written by our own ZIP writer, so the reader is
//! exercised end to end rather than against a mock.

use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::epub::{Epub, NavNode, NavSource};
use crate::format::{BookFormat, detect_format};
use crate::txt::{TextEncoding, TxtDocument, chapter_title};
use crate::xhtml::{BlockKind, extract_blocks};
use crate::zip::{Method, ZipArchive, ZipError, ZipLimits};
use crate::zip_writer::ZipWriter;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A temporary file removed when the test ends.
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    fn new(suffix: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "reader-platform-{}-{}-{suffix}",
            std::process::id(),
            id
        ));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        let mut file = std::fs::File::create(&self.path)?;
        file.write_all(bytes)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

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
    <dc:creator>Alan Turing</dc:creator>
    <dc:language>en</dc:language>
    <dc:identifier id="bookid">urn:uuid:1234</dc:identifier>
    <dc:publisher>Test Press</dc:publisher>
    <meta property="dcterms:modified">2026-09-15T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="text/ch2.xhtml" media-type="application/xhtml+xml"/>
    <item id="cover" href="images/cover.png" media-type="image/png" properties="cover-image"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
    <itemref idref="ch2" properties="rendition:page-spread-right"/>
  </spine>
</package>"#;

const NAV_XHTML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
  <head><title>Contents</title></head>
  <body>
    <nav epub:type="toc">
      <ol>
        <li><a href="text/ch1.xhtml">Chapter 1</a></li>
        <li><a href="text/ch2.xhtml#top">Chapter 2</a>
          <ol>
            <li><a href="text/ch2.xhtml#part2">Part 2</a></li>
          </ol>
        </li>
      </ol>
    </nav>
  </body>
</html>"#;

const CH1_XHTML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head><title>Chapter 1</title><style>p { color: red; }</style></head>
  <body>
    <h1>Chapter 1</h1>
    <p>The brave sailor set sail.</p>
    <script>var ignored = true;</script>
    <p>Second <em>paragraph</em>.</p>
  </body>
</html>"#;

const CH2_XHTML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <body>
    <h1 id="top">Chapter 2</h1>
    <p>The storm arrived.</p>
  </body>
</html>"#;

fn write_epub(path: &Path) -> Vec<u8> {
    let cover = vec![0x89u8, b'P', b'N', b'G', 1, 2, 3, 4];
    let mut writer = ZipWriter::new(std::fs::File::create(path).expect("create fixture"));
    writer.add_stored("mimetype", b"application/epub+zip").expect("mimetype");
    writer
        .add_deflated("META-INF/container.xml", CONTAINER_XML.as_bytes())
        .expect("container");
    writer
        .add_deflated("OEBPS/content.opf", PACKAGE_OPF.as_bytes())
        .expect("opf");
    writer
        .add_deflated("OEBPS/nav.xhtml", NAV_XHTML.as_bytes())
        .expect("nav");
    writer
        .add_deflated("OEBPS/text/ch1.xhtml", CH1_XHTML.as_bytes())
        .expect("ch1");
    writer
        .add_deflated("OEBPS/text/ch2.xhtml", CH2_XHTML.as_bytes())
        .expect("ch2");
    writer
        .add_deflated("OEBPS/images/cover.png", &cover)
        .expect("cover");
    writer.finish().expect("finish");
    cover
}

#[test]
fn zip_writer_and_reader_round_trip() {
    let file = TempFile::new("round-trip.zip");
    let mut writer = ZipWriter::new(std::fs::File::create(file.path()).expect("create"));
    writer.add_stored("stored.txt", b"stored payload").expect("store");
    writer
        .add_deflated("deflated.txt", b"deflated payload deflated payload")
        .expect("deflate");
    writer.finish().expect("finish");

    let mut archive = ZipArchive::open(std::fs::File::open(file.path()).expect("open")).expect("archive");
    assert_eq!(archive.entries().len(), 2);
    let stored = archive.find("stored.txt").expect("stored entry").clone();
    assert_eq!(stored.method, Method::Stored);
    assert_eq!(archive.read_entry(&stored).expect("read"), b"stored payload");

    let deflated = archive.find("deflated.txt").expect("deflated entry").clone();
    assert_eq!(deflated.method, Method::Deflate);
    assert_eq!(
        archive.read_entry(&deflated).expect("read"),
        b"deflated payload deflated payload"
    );
}

#[test]
fn zip_rejects_path_traversal_and_unsafe_names() {
    let file = TempFile::new("traversal.zip");
    let mut writer = ZipWriter::new(std::fs::File::create(file.path()).expect("create"));
    writer.add_stored("../evil.txt", b"nope").expect("write");
    writer.finish().expect("finish");

    let error = ZipArchive::open(std::fs::File::open(file.path()).expect("open")).unwrap_err();
    assert!(matches!(error, ZipError::UnsafeName(_)), "got {error:?}");
}

#[test]
fn zip_detects_a_corrupted_entry() {
    let file = TempFile::new("corrupt.zip");
    let mut writer = ZipWriter::new(std::fs::File::create(file.path()).expect("create"));
    writer.add_stored("a.txt", b"0123456789").expect("write");
    writer.finish().expect("finish");

    // Corrupt one data byte inside the entry: 30 byte header + 5 byte name.
    let mut handle = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(file.path())
        .expect("reopen");
    handle.seek(SeekFrom::Start(30 + 5 + 2)).expect("seek");
    handle.write_all(&[0xFF]).expect("corrupt");
    handle.sync_all().expect("sync");

    let mut archive = ZipArchive::open(std::fs::File::open(file.path()).expect("open")).expect("archive");
    let entry = archive.find("a.txt").expect("entry").clone();
    assert!(matches!(
        archive.read_entry(&entry).unwrap_err(),
        ZipError::Integrity { .. }
    ));
}

#[test]
fn zip_enforces_size_limits() {
    let file = TempFile::new("limits.zip");
    let mut writer = ZipWriter::new(std::fs::File::create(file.path()).expect("create"));
    writer.add_stored("big.txt", &[b'x'; 64]).expect("write");
    writer.finish().expect("finish");

    let limits = ZipLimits {
        max_entry_size: 16,
        ..ZipLimits::default()
    };
    let error = ZipArchive::with_limits(std::fs::File::open(file.path()).expect("open"), limits)
        .unwrap_err();
    assert!(matches!(error, ZipError::LimitExceeded(_)), "got {error:?}");
}

fn nav_labels(nodes: &[NavNode]) -> Vec<String> {
    nodes.iter().map(|node| node.label.clone()).collect()
}

#[test]
fn epub_parses_package_navigation_and_sections() {
    let file = TempFile::new("book.epub");
    let cover = write_epub(file.path());
    let mut epub = Epub::open(file.path(), "edition-1").expect("open epub");

    {
        let package = epub.package();
        assert_eq!(package.opf_path, "OEBPS/content.opf");
        assert_eq!(package.metadata.title.as_deref(), Some("Test Book"));
        assert_eq!(package.metadata.authors, vec!["Ada Lovelace", "Alan Turing"]);
        assert_eq!(package.metadata.language.as_deref(), Some("en"));
        assert_eq!(package.metadata.publisher.as_deref(), Some("Test Press"));
        assert_eq!(
            package.metadata.extra.get("meta").map(Vec::len),
            Some(1),
            "unknown metadata elements are kept"
        );
        assert_eq!(package.cover_href(), Some("OEBPS/images/cover.png"));
        assert_eq!(package.spine.len(), 2);
        assert_eq!(package.spine[0].href, "OEBPS/text/ch1.xhtml");
        assert_eq!(package.spine[1].ordinal, 1);
        assert_eq!(package.nav_source, Some(NavSource::Nav));
        assert_eq!(nav_labels(&package.nav), vec!["Chapter 1", "Chapter 2"]);
        assert_eq!(package.nav[1].fragment.as_deref(), Some("top"));
        assert_eq!(nav_labels(&package.nav[1].children), vec!["Part 2"]);
        assert_eq!(package.nav[1].children[0].fragment.as_deref(), Some("part2"));
    }

    let first = epub.section(0).expect("section 0");
    assert_eq!(first.section_id, "OEBPS/text/ch1.xhtml");
    assert_eq!(first.spine_index, Some(0));
    assert_eq!(
        first
            .blocks
            .iter()
            .map(|block| block.text.clone())
            .collect::<Vec<_>>(),
        vec![
            "Chapter 1".to_string(),
            "The brave sailor set sail.".to_string(),
            "Second paragraph.".to_string(),
        ]
    );
    assert!(first.doc_fingerprint.as_deref().unwrap_or_default().starts_with("sha256-"));
    assert!(first.blocks.iter().all(|block| block.block_id.starts_with("block-")));

    let second = epub.section(1).expect("section 1");
    assert_eq!(second.blocks.len(), 2);

    let all = epub.index_all(0).expect("index all");
    assert_eq!(all.len(), 2);
    assert_eq!(epub.resource("OEBPS/images/cover.png").expect("cover"), cover);

    // Reopening produces identical derived IDs: the index is rebuildable.
    let mut reopened = Epub::open(file.path(), "edition-1").expect("reopen");
    let first_again = reopened.section(0).expect("section 0 again");
    assert_eq!(first, first_again);
}

#[test]
fn epub_section_kinds_are_preserved() {
    let file = TempFile::new("kinds.epub");
    write_epub(file.path());
    let mut epub = Epub::open(file.path(), "edition-1").expect("open");
    let section = epub.section(0).expect("section");
    assert_eq!(section.blocks.len(), 3);
    // Titles are read from the derived text index, so the blocks themselves
    // carry no kind; kinds come from the XHTML extractor.
    let html = String::from_utf8(epub.resource("OEBPS/text/ch1.xhtml").expect("resource")).expect("utf8");
    let blocks = extract_blocks(&html).expect("blocks");
    assert_eq!(
        blocks.iter().map(|block| block.kind).collect::<Vec<_>>(),
        vec![BlockKind::Heading, BlockKind::Paragraph, BlockKind::Paragraph]
    );
}

#[test]
fn xhtml_extraction_skips_non_book_content() {
    let html = r#"<html><head><title>Hidden</title><style>p{}</style></head>
        <body><h2>Title</h2><p>One<br/>two</p><ul><li>Item</li></ul>
        <script>alert('x')</script><blockquote>Quoted</blockquote></body></html>"#;
    let blocks = extract_blocks(html).expect("blocks");
    let texts: Vec<String> = blocks.iter().map(|block| block.text.clone()).collect();
    assert_eq!(
        texts,
        vec![
            "Title".to_string(),
            "One two".to_string(),
            "Item".to_string(),
            "Quoted".to_string(),
        ]
    );
    assert!(!texts.iter().any(|text| text.contains("Hidden") || text.contains("alert")));
    assert_eq!(blocks[0].kind, BlockKind::Heading);
    assert_eq!(blocks[1].kind, BlockKind::Paragraph);
    assert_eq!(blocks[2].kind, BlockKind::ListItem);
    assert_eq!(blocks[3].kind, BlockKind::Quote);
}

const NOVEL: &str = "书名：测试小说\n作者：某人\n\n第一章 开始\n正文第一段。\n\n正文第二段。\n第二章 继续\n更多内容。\n\nChapter 3 The End\nfin.\n";

#[test]
fn txt_indexes_chapters_and_reads_them_back() {
    let file = TempFile::new("novel.txt");
    file.write(NOVEL.as_bytes()).expect("write");
    let document = TxtDocument::open(file.path()).expect("open txt");

    assert_eq!(document.encoding().encoding, TextEncoding::Utf8);
    assert!(!document.encoding().assumed);
    let titles: Vec<&str> = document
        .chapters()
        .iter()
        .map(|chapter| chapter.title.as_str())
        .collect();
    assert_eq!(titles, vec!["第一章 开始", "第二章 继续", "Chapter 3 The End"]);
    assert_eq!(document.total_bytes(), NOVEL.len() as u64);

    let first = document.read_chapter(0).expect("chapter 0");
    assert!(first.starts_with("第一章 开始"));
    assert!(first.contains("正文第二段。"));
    assert!(!first.contains("第二章"));

    let third = document.read_chapter(2).expect("chapter 2");
    assert!(third.contains("fin."));
    assert!(!third.contains("第二章"));

    let blocks = document.chapter_blocks("edition-1", 0).expect("blocks");
    assert_eq!(
        blocks.len(),
        2,
        "paragraphs split on blank lines; consecutive lines stay together"
    );
    assert!(blocks[0].text.contains("正文第一段。"));
    assert!(blocks[1].text.contains("正文第二段。"));
    assert!(blocks[0].block_id.starts_with("block-"));
}

#[test]
fn txt_without_chapter_markers_becomes_one_chapter() {
    let file = TempFile::new("flat.txt");
    file.write(b"just some text\nwithout chapters\n").expect("write");
    let document = TxtDocument::open(file.path()).expect("open");
    assert_eq!(document.chapters().len(), 1);
    let title = document.chapters()[0].title.clone();
    assert!(title.starts_with("reader-platform-"), "title falls back to the file name, got {title}");
    assert!(document.read_chapter(0).expect("read").contains("just some text"));
}

#[test]
fn txt_detects_gbk_and_utf16() {
    let gbk_file = TempFile::new("gbk.txt");
    let (gbk_bytes, _, had_errors) = encoding_rs::GBK.encode("第一章 开始\n中文内容。\n");
    assert!(!had_errors);
    gbk_file.write(&gbk_bytes).expect("write gbk");
    let gbk = TxtDocument::open(gbk_file.path()).expect("open gbk");
    assert_eq!(gbk.encoding().encoding, TextEncoding::Gbk);
    assert!(gbk.encoding().assumed, "GBK is a guess and must be labelled");
    assert_eq!(gbk.chapters()[0].title, "第一章 开始");
    assert!(gbk.read_chapter(0).expect("read").contains("中文内容。"));

    let utf16_file = TempFile::new("utf16.txt");
    let text = "第一章 开始\nUTF-16 content\n";
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    utf16_file.write(&bytes).expect("write utf16");
    let utf16 = TxtDocument::open(utf16_file.path()).expect("open utf16");
    assert_eq!(utf16.encoding().encoding, TextEncoding::Utf16Le);
    assert!(!utf16.encoding().assumed);
    assert_eq!(utf16.chapters()[0].title, "第一章 开始");
    assert!(utf16.read_chapter(0).expect("read").contains("UTF-16 content"));
}

#[test]
fn chapter_title_detection_is_conservative() {
    assert_eq!(chapter_title("第一章 开始").as_deref(), Some("第一章 开始"));
    assert_eq!(chapter_title("Chapter 12").as_deref(), Some("Chapter 12"));
    assert_eq!(chapter_title("序章").as_deref(), Some("序章"));
    assert_eq!(chapter_title("这是一句普通的正文。"), None);
    assert_eq!(chapter_title("第一人称视角"), None);
    assert_eq!(
        chapter_title(&"x".repeat(80)),
        None,
        "very long lines are body text, not headings"
    );
    assert_eq!(
        chapter_title("第一章 这是一个非常非常长的标题：{}".replace("{}", &"很".repeat(60)).as_str()),
        None
    );
}

#[test]
fn format_detection_uses_content_not_extension() {
    let epub = TempFile::new("mystery.txt");
    write_epub(epub.path());
    assert_eq!(detect_format(epub.path()).expect("detect"), BookFormat::Epub);

    let pdf = TempFile::new("document.txt");
    pdf.write(b"%PDF-1.7\n...").expect("write");
    assert_eq!(detect_format(pdf.path()).expect("detect"), BookFormat::Pdf);

    let html = TempFile::new("page.txt");
    html.write(b"<!DOCTYPE html><html><body>hi</body></html>").expect("write");
    assert_eq!(detect_format(html.path()).expect("detect"), BookFormat::Html);

    let text = TempFile::new("plain.txt");
    text.write(b"just text\n").expect("write");
    assert_eq!(detect_format(text.path()).expect("detect"), BookFormat::Txt);
}

#[test]
fn txt_open_bytes_parses_without_filesystem() {
    let bytes: Vec<u8> = "\
第一章 冒险开始
水手登上了船，准备开始他们的旅程。

第二章 风暴
海浪高高地掀起，雨打在甲板上。
"
    .into();
    let doc = TxtDocument::open_bytes(bytes.clone(), Some("adventure")).expect("open_bytes");
    assert_eq!(doc.chapters().len(), 2);
    assert_eq!(doc.chapters()[0].title, "第一章 冒险开始");
    assert_eq!(doc.chapters()[1].title, "第二章 风暴");
    assert_eq!(doc.source_name(), Some("adventure"));
    assert_eq!(doc.total_bytes(), bytes.len() as u64);

    let first = doc.read_chapter(0).expect("read ch0");
    assert!(first.contains("冒险开始"));
    assert!(first.contains("水手登上了船"));
    let second = doc.read_chapter(1).expect("read ch1");
    assert!(second.contains("风暴"));
    assert!(second.contains("海浪高高地掀起"));

    let blocks = doc.chapter_blocks("edition-1", 0).expect("blocks");
    assert!(!blocks.is_empty(), "chapter 0 should produce at least one block");
}

#[test]
fn epub_open_reader_from_memory_round_trips() {
    let cover = vec![0x89u8, b'P', b'N', b'G', 1, 2, 3, 4];
    let mut writer = ZipWriter::new(Vec::new());
    writer.add_stored("mimetype", b"application/epub+zip").unwrap();
    writer
        .add_deflated("META-INF/container.xml", CONTAINER_XML.as_bytes())
        .unwrap();
    writer
        .add_deflated("OEBPS/content.opf", PACKAGE_OPF.as_bytes())
        .unwrap();
    writer
        .add_deflated("OEBPS/nav.xhtml", NAV_XHTML.as_bytes())
        .unwrap();
    writer
        .add_deflated("OEBPS/text/ch1.xhtml", CH1_XHTML.as_bytes())
        .unwrap();
    writer
        .add_deflated("OEBPS/text/ch2.xhtml", CH2_XHTML.as_bytes())
        .unwrap();
    writer
        .add_deflated("OEBPS/images/cover.png", &cover)
        .unwrap();
    let bytes = writer.finish().expect("finish");
    assert!(!bytes.is_empty());

    let cursor = std::io::Cursor::new(bytes);
    let mut epub: Epub<std::io::Cursor<Vec<u8>>> =
        Epub::open_reader(cursor, "edition-mem").expect("open_reader");

    assert_eq!(epub.edition_id(), "edition-mem");
    assert_eq!(epub.package().manifest.len(), 4);
    assert_eq!(epub.package().spine.len(), 2);

    let section = epub.section(0).expect("section 0");
    assert_eq!(section.section_id, "OEBPS/text/ch1.xhtml");
    assert!(!section.blocks.is_empty());
    assert!(section
        .blocks
        .iter()
        .any(|b| b.text.contains("brave sailor")));

    let section2 = epub.section(1).expect("section 1");
    assert_eq!(section2.section_id, "OEBPS/text/ch2.xhtml");
    assert!(section2
        .blocks
        .iter()
        .any(|b| b.text.contains("storm arrived")));
}
