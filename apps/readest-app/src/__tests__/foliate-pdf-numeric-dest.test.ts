// A destination's first item is normally an indirect reference to the page
// object, but PDF 32000-1 §12.3.2.2 also allows the page-number form (0-based,
// what remote go-to actions use), and generators emit it in local outlines too.
// pdf.js' own viewer accepts it -- `getPageIndex()` does not, it only takes
// references -- so `makePDF` has to translate it itself. Unhandled, every
// outline entry of such a PDF resolves to no page at all: sidebar clicks log
// "Could not go to ...", and `TOCProgress` groups the whole TOC under `null`,
// so the progress bar and page footer point at the wrong chapter.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { TOCProgress } from 'foliate-js/progress.js';

interface OutlineItem {
  title: string;
  dest: unknown;
  items: OutlineItem[] | null;
}

interface TOCItem {
  label: string;
  href: string;
  index?: number;
}

interface PdfBook {
  toc: TOCItem[] | null;
  sections: { id: number }[];
  resolveHref: (href: string) => Promise<{ index: number }>;
  splitTOCHref: (href: string) => Promise<[number | null, string | null]>;
  getTOCFragment: (doc: Document) => Element;
}

let outline: OutlineItem[] | null = null;
let destinations = new Map<string, unknown>();
let numPages = 8;

// The same duck-typed check the vendored pdf.js build uses.
const isRefProxy = (ref: unknown): ref is { num: number; gen: number } =>
  typeof ref === 'object' &&
  Number.isInteger((ref as { num?: unknown })?.num) &&
  Number.isInteger((ref as { gen?: unknown })?.gen);

vi.mock('@pdfjs/pdf.min.mjs', () => {
  class PDFDataRangeTransport {
    requestDataRange!: (begin: number, end: number) => void;
    onDataRange = vi.fn();
    constructor(
      public length: number,
      public initialData: unknown,
    ) {}
  }
  const getDocument = vi.fn(() => ({
    promise: Promise.resolve({
      get numPages() {
        return numPages;
      },
      getPage: vi.fn(async () => ({
        getViewport: () => ({ width: 600, height: 800 }),
        cleanup: vi.fn(),
      })),
      getMetadata: vi.fn(async () => ({ metadata: undefined, info: {} })),
      getViewerPreferences: vi.fn(async () => null),
      getOutline: vi.fn(async () => outline),
      getPageLabels: vi.fn(async () => null),
      getDestination: vi.fn(async (id: string) => destinations.get(id) ?? null),
      getPageIndex: vi.fn(async (ref: unknown) => {
        if (!isRefProxy(ref)) throw new Error('Invalid pageIndex request.');
        return ref.num;
      }),
      destroy: vi.fn(),
    }),
    destroy: vi.fn(),
  }));
  (globalThis as unknown as { pdfjsLib: unknown }).pdfjsLib = {
    GlobalWorkerOptions: {},
    PDFDataRangeTransport,
    getDocument,
  };
  return {};
});

const fakeFile = {
  size: 1,
  slice: () => ({ arrayBuffer: async () => new ArrayBuffer(0) }),
} as unknown as File;

const open = async (toc: OutlineItem[] | null, pages = 8) => {
  outline = toc;
  numPages = pages;
  destinations = new Map();
  const { makePDF } = await import('foliate-js/pdf.js');
  return (await makePDF(fakeFile)) as unknown as PdfBook;
};

const firstItem = (book: PdfBook) => {
  const item = book.toc?.[0];
  if (!item) throw new Error('expected the book to have a TOC item');
  return item;
};

// An outline entry the way this class of PDF writes it: page number, not ref.
const numericDest = (pageIndex: number) => [pageIndex, { name: 'XYZ' }, null, 359.80688, null];

// The reader's own pipeline: `View.open` builds TOCProgress from these hooks.
const tocItemFor = async (book: PdfBook, index: number) => {
  const progress = new TOCProgress();
  await progress.init({
    toc: book.toc ?? [],
    ids: book.sections.map((s) => s.id),
    splitHref: book.splitTOCHref.bind(book),
    getFragment: book.getTOCFragment.bind(book),
  });
  return progress.getProgress(index, undefined) as TOCItem | null | undefined;
};

afterEach(() => {
  vi.restoreAllMocks();
});

describe('makePDF TOC destinations', () => {
  it('resolves a destination that names its page by 0-based page number', async () => {
    const book = await open([{ title: '四、产品核心能力', dest: numericDest(4), items: null }]);
    const item = firstItem(book);
    expect(item.index).toBe(4);
    expect(await book.splitTOCHref(item.href)).toEqual([4, null]);
    expect(await book.resolveHref(item.href)).toEqual({ index: 4 });
  });

  it('maps a relocate to the TOC entry of the page it landed on', async () => {
    const book = await open([
      { title: '一、项目概述', dest: numericDest(0), items: null },
      { title: '二、核心产品定义', dest: numericDest(1), items: null },
      { title: '四、产品核心能力', dest: numericDest(4), items: null },
    ]);
    expect((await tocItemFor(book, 0))?.label).toBe('一、项目概述');
    expect((await tocItemFor(book, 2))?.label).toBe('二、核心产品定义');
    expect((await tocItemFor(book, 5))?.label).toBe('四、产品核心能力');
  });

  it('resolves a named destination whose target is a page number', async () => {
    const book = await open([{ title: 'Chapter', dest: 'ch1', items: null }]);
    destinations.set('ch1', numericDest(6));
    const item = firstItem(book);
    expect(await book.splitTOCHref(item.href)).toEqual([6, null]);
    expect(await book.resolveHref(item.href)).toEqual({ index: 6 });
  });

  it('still resolves an indirect reference through pdf.js', async () => {
    const dest = [{ num: 3, gen: 0 }, { name: 'XYZ' }, null, 120, null];
    const book = await open([{ title: 'Chapter', dest, items: null }]);
    const item = firstItem(book);
    expect(item.index).toBe(3);
    expect(await book.splitTOCHref(item.href)).toEqual([3, null]);
    expect(await book.resolveHref(item.href)).toEqual({ index: 3 });
  });

  it('leaves a destination it cannot place unresolved instead of breaking the TOC', async () => {
    const book = await open([
      { title: 'Missing', dest: 'no-such-dest', items: null },
      { title: 'Out of range', dest: numericDest(99), items: null },
    ]);
    const [missing, outOfRange] = book.toc ?? [];
    expect(missing?.index).toBeUndefined();
    expect(await book.splitTOCHref(missing?.href ?? '')).toEqual([null, null]);
    expect(await book.splitTOCHref(outOfRange?.href ?? '')).toEqual([null, null]);
  });
});
