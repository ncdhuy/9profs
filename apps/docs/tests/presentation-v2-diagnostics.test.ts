import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parseDocx, readSections, saveDocx } from '@genoffice/docx-engine'
import {
  buildDocx,
  buildKitchenSinkDocx,
  NESTED_TABLE_XML,
  REVISION_TABLE_XML,
} from '../../../packages/docx-engine/tests/helpers/build-docx'
import { blocksToPmDoc, pmDocToSavePlan } from '../src/renderer/editor/convert'
import type { DocDirtyState } from '../src/renderer/doc-dirty'
import { computeLineMetrics } from '../src/renderer/line-metrics'
import {
  assignSections,
  sectionGeoms,
  type BlockBox,
  type SectionGeom,
} from '../src/renderer/pagination'
import {
  captureEditorPositionDiagnostics,
  captureModelDiagnostics,
  capturePostRenderDiagnostics,
  capturePresentationDiagnostics,
  compareDiagnosticParity,
  formatDiagnosticDiffs,
  type PresentationDiagnosticSource,
} from '../src/renderer/presentation-v2'
import { renderPresentation, type PresentationRenderer } from '../src/renderer/presentation-v2'

const __dirname = dirname(fileURLToPath(import.meta.url))
const CORPUS = join(__dirname, 'pagination-corpus/docx')

const HEADER_XML =
  '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
  '<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Header</w:t></w:r></w:p></w:hdr>'
const FOOTER_XML =
  '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
  '<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Footer</w:t></w:r></w:p></w:ftr>'
const COMMENTS_XML =
  '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
  '<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:comment w:id="1" w:author="Test"><w:p><w:r><w:t>Comment</w:t></w:r></w:p></w:comment></w:comments>'

type Fixture = { name: string; load: () => Promise<Uint8Array> }

const staticFixture = (name: string, file: string): Fixture => ({
  name,
  load: async () => new Uint8Array(readFileSync(join(CORPUS, file))),
})

const generatedFixture = (name: string, load: () => Promise<Uint8Array>): Fixture => ({
  name,
  load,
})

const FIXTURES: Fixture[] = [
  staticFixture('simple prose', '01-simple-english.docx'),
  staticFixture('headings / keep-next', '04-headings-keepnext.docx'),
  staticFixture('mixed fonts / language', '03-mixed-lang.docx'),
  staticFixture('CJK / doc-grid', '02-chinese-long-docgrid.docx'),
  staticFixture('explicit page breaks', '07-page-breaks.docx'),
  staticFixture('sections with different page geometry', '08-multi-section-paper.docx'),
  staticFixture('multi-column document', '22-two-columns.docx'),
  staticFixture('normal table', '11-multi-tables.docx'),
  staticFixture('long table', '05-long-table.docx'),
  staticFixture('footnotes', '06-with-footnotes.docx'),
  generatedFixture('nested table', async () => buildDocx({ bodyXml: NESTED_TABLE_XML })),
  generatedFixture('header/footer', async () =>
    buildDocx({
      bodyXml: '<w:p><w:r><w:t>Body</w:t></w:r></w:p>',
      sectPrExtra:
        '<w:headerReference w:type="default" r:id="rId20"/><w:footerReference w:type="default" r:id="rId21"/>',
      extraRels:
        '<Relationship Id="rId20" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>' +
        '<Relationship Id="rId21" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>',
      extraParts: [
        {
          path: 'word/header1.xml',
          xml: HEADER_XML,
          contentType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml',
        },
        {
          path: 'word/footer1.xml',
          xml: FOOTER_XML,
          contentType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml',
        },
      ],
    }),
  ),
  generatedFixture('floating image / drawing / text box', buildKitchenSinkDocx),
  generatedFixture('comments', async () =>
    buildDocx({
      bodyXml:
        '<w:p><w:commentRangeStart w:id="1"/><w:r><w:t>Commented</w:t></w:r><w:commentRangeEnd w:id="1"/><w:r><w:commentReference w:id="1"/></w:r></w:p>',
      extraRels:
        '<Relationship Id="rId30" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/>',
      extraParts: [
        {
          path: 'word/comments.xml',
          xml: COMMENTS_XML,
          contentType:
            'application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml',
        },
      ],
    }),
  ),
  generatedFixture('revisions', async () => buildDocx({ bodyXml: REVISION_TABLE_XML })),
  generatedFixture('unsupported/raw-content preservation', async () =>
    buildDocx({
      bodyXml:
        '<w:p><w:sdt><w:sdtPr><w:alias w:val="raw"/></w:sdtPr><w:sdtContent><w:r><w:t>Raw content</w:t></w:r></w:sdtContent></w:sdt></w:p>',
    }),
  ),
]

function cleanDirtyState(): DocDirtyState {
  return {
    dirtyRef: { current: false },
    sectionDirty: false,
    sectionsDirty: [],
    trailingStartType: null,
    pageColorDirty: false,
    headerDirty: false,
    footerDirty: false,
    hfVariantsDirty: [],
    sectionHfEdits: {},
    pgNumEdit: null,
    pgNumDirtySections: [],
    numberingDirty: false,
    styleUpserts: {},
    titlePgDirty: false,
    evenOddHfDirty: false,
    watermarkDirty: false,
    inksDirty: false,
    notesDirty: false,
    sourcesDirty: false,
    themeFontsDirty: false,
    themeColorsDirty: false,
    commentsDirty: false,
    protectionDirty: false,
  }
}

function textOf(block: { runs?: Array<{ text?: string }> }): string {
  return (block.runs ?? []).map((run) => run.text ?? '').join('')
}

function blockBoxes(parsed: Awaited<ReturnType<typeof parseDocx>>): BlockBox[] {
  let top = 0
  const blocks: BlockBox[] = parsed.blocks.map((block) => {
    const raw = block as typeof block & {
      table?: { rows?: unknown[] }
      originalXml?: string
      format?: {
        pageBreakBefore?: boolean
        keepNext?: boolean
        keepLines?: boolean
        widowControl?: boolean
      }
    }
    const text = textOf(raw)
    const metrics = computeLineMetrics({
      runs: text ? [{ text }] : [],
      availWidthPx: 520,
      defaultFontSizePt: 12,
      isEmpty: !text,
    })
    const tableRowCount = raw.table?.rows?.length ?? 0
    const tableRows =
      tableRowCount > 0
        ? Array.from({ length: tableRowCount }, (_, row) => ({
            height: 28,
            isHeader: row === 0 && /<w:tblHeader\b/.test(raw.originalXml ?? ''),
            cutYs: [14],
          }))
        : undefined
    const height = tableRows
      ? tableRows.reduce((sum, row) => sum + row.height, 0)
      : Math.max(24, metrics.totalHeight)
    const lineBoxes = tableRows
      ? undefined
      : metrics.lineBoxes.map((line) => ({
          offsetInBlock: line.offsetInBlock,
          height: line.height,
        }))
    const box: BlockBox = {
      top,
      height,
      docxIndex: block.docxIndex ?? undefined,
      breakBefore: raw.format?.pageBreakBefore,
      keepNext: raw.format?.keepNext,
      keepLines: raw.format?.keepLines,
      widowControl: raw.format?.widowControl,
      ...(lineBoxes ? { lineBoxes, lineOffsets: lineBoxes.map((line) => line.offsetInBlock) } : {}),
      ...(tableRows ? { tableRows, fixedWidthPx: 520 } : {}),
      ...(raw.originalXml && /<wp:anchor\b|<w:tblpPr\b/.test(raw.originalXml)
        ? { floated: true }
        : {}),
      el: document.createElement('div'),
    }
    top += height
    return box
  })
  return blocks
}

function gapInputs(
  slices: ReturnType<typeof renderPresentation>,
  sections: Awaited<ReturnType<typeof readSections>>,
) {
  return slices.slice(1).map((slice, index) => {
    const settings = sections[slice.section]?.settings ?? sections[0]?.settings
    const twips = (value: number | undefined) => (value ?? 0) / 15
    return {
      page: index + 2,
      kind: slice.repeatHeader ? ('table' as const) : ('block' as const),
      metrics: {
        marginTop: twips(settings?.marginTop),
        marginBottom: twips(settings?.marginBottom),
        marginLeft: twips(settings?.marginLeft),
        marginRight: twips(settings?.marginRight),
      },
      ...(slice.repeatHeader ? { hasRepeatedHeader: true } : {}),
    }
  })
}

function sourceFor(
  parsed: Awaited<ReturnType<typeof parseDocx>>,
  sections: Awaited<ReturnType<typeof readSections>>,
  blocks: BlockBox[],
  geoms: SectionGeom[],
  slices: ReturnType<typeof renderPresentation>,
): PresentationDiagnosticSource {
  const hfParts = Object.keys(parsed.hfParts ?? {})
  const hasHf = hfParts.length > 0
  const headerFooters = hasHf
    ? slices.flatMap((_, page) => [
        { page: page + 1, kind: 'header' as const, source: hfParts.join(',') },
        { page: page + 1, kind: 'footer' as const, source: hfParts.join(',') },
      ])
    : []
  return {
    blocks,
    sectionGeoms: geoms,
    slices,
    sections,
    pageGaps: gapInputs(slices, sections),
    headerFooters,
    floatShifts: blocks
      .map((block, index) => (block.floated ? { block: block.docxIndex ?? index, dy: 0 } : null))
      .filter((item): item is { block: number; dy: number } => item !== null),
  }
}

async function runFixture(bytes: Uint8Array, renderer: PresentationRenderer) {
  const parsed = await parseDocx(bytes)
  const sections = readSections(parsed)
  const blocks = blockBoxes(parsed)
  assignSections(blocks, sections)
  const geoms = sectionGeoms(
    sections,
    sections.map(() => ({ headerPx: 0, footerPx: 0 })),
  )
  const input = {
    blocks,
    sectionGeoms: geoms,
    totalHeight: Math.max(...blocks.map((block) => block.top + block.height), 1),
    zoomFactor: 1,
  }
  const slices = renderPresentation(renderer, input)
  const pmDoc = blocksToPmDoc(parsed.blocks, sections)
  const savePlan = pmDocToSavePlan(pmDoc, parsed.blocks)
  const savedBytes = await saveDocx(parsed, savePlan.saveBlocks)
  const reopened = await parseDocx(savedBytes)
  const presentation = capturePresentationDiagnostics(
    sourceFor(parsed, sections, blocks, geoms, slices),
  )
  const model = captureModelDiagnostics({
    pmJson: pmDoc,
    selection: { anchor: 1, head: 1, from: 1, to: 1 },
    dirtyState: cleanDirtyState(),
    savePlan,
    savedBytes,
    reopenedPmJson: blocksToPmDoc(reopened.blocks, readSections(reopened)),
    reopenedSelection: { anchor: 1, head: 1, from: 1, to: 1 },
  })
  return { presentation, model, savedBytes, sourceBytes: bytes }
}

describe('DOCX V1/V2 presentation diagnostics harness', () => {
  for (const fixture of FIXTURES) {
    it(`matches V1/V2 for ${fixture.name}`, async () => {
      const bytes = await fixture.load()
      const v1 = await runFixture(bytes, 'v1')
      const v2 = await runFixture(bytes, 'v2')
      const differences = compareDiagnosticParity(
        { presentation: v1.presentation, model: v1.model },
        { presentation: v2.presentation, model: v2.model },
        { fixture: fixture.name },
      )
      if (differences.length > 0) throw new Error(formatDiagnosticDiffs(differences))
      expect(v1.presentation.pageCount).toBe(v2.presentation.pageCount)
      expect(v1.model.dirty).toBe(false)
      expect(v1.model.saveOutput).toEqual({ kind: 'bytes', bytes: Array.from(bytes) })
      expect(v1.model.reopenedPmJson).toEqual(v1.model.pmJson)
    })
  }

  it('captures current position-to-rectangle and rectangle-to-position APIs', () => {
    const mapping = captureEditorPositionDiagnostics(
      {
        coordsAtPos: (position) => ({
          left: position * 2,
          top: 10,
          right: position * 2 + 1,
          bottom: 24,
        }),
        posAtCoords: ({ left }) => ({ pos: left / 2 }),
      },
      [1, 4],
    )
    expect(mapping).toEqual([
      {
        position: 1,
        rect: { left: 2, top: 10, right: 3, bottom: 24, width: 1, height: 14 },
        hitPosition: 1,
        roundTrip: true,
      },
      {
        position: 4,
        rect: { left: 8, top: 10, right: 9, bottom: 24, width: 1, height: 14 },
        hitPosition: 4,
        roundTrip: true,
      },
    ])
  })

  it('normalizes page-gap, header/footer, float, caret, and selection side effects', () => {
    const diagnostics = capturePresentationDiagnostics({
      blocks: [
        {
          top: 0,
          height: 30,
          docxIndex: 9,
          lineBoxes: [{ offsetInBlock: 0, height: 20 }],
        },
      ],
      sectionGeoms: [{ contentHeight: 700, forceBreak: false }],
      slices: [{ start: 0, end: 30, section: 0 }],
      pageGaps: [
        {
          page: 2,
          kind: 'cut',
          metrics: { marginTop: 10, marginBottom: 12, marginLeft: 14, marginRight: 16 },
        },
      ],
      headerFooters: [
        {
          page: 1,
          kind: 'header',
          variant: 'default',
          rect: { left: 10, top: 20, width: 100, height: 12 },
        },
      ],
      floatShifts: [{ page: 1, block: 9, dy: 4.5 }],
      positionMappings: [{ position: 1, hitPosition: 1, roundTrip: true }],
      caret: { left: 1, top: 2, width: 1, height: 14 },
      selection: [{ left: 1, top: 2, width: 20, height: 14 }],
    })
    expect(diagnostics.pageGaps[0]).toMatchObject({ page: 2, kind: 'cut', height: 0 })
    expect(diagnostics.headerFooters[0].rect).toEqual({
      left: 10,
      top: 20,
      width: 100,
      height: 12,
    })
    expect(diagnostics.floats[0]).toMatchObject({ page: 1, block: 9, dy: 4.5 })
    expect(diagnostics.caret).toEqual({ left: 1, top: 2, width: 1, height: 14 })
    expect(diagnostics.selection).toEqual([{ left: 1, top: 2, width: 20, height: 14 }])
  })

  it('reports structured category/page/block diagnostics on parity failure', () => {
    const differences = compareDiagnosticParity(
      { pageCount: 1, lines: [{ page: 1, block: 7, height: 20 }] },
      { pageCount: 2, lines: [{ page: 1, block: 7, height: 20.02 }] },
      { fixture: 'diagnostic-fixture' },
    )
    const report = formatDiagnosticDiffs(differences)
    expect(report).toContain('fixture=diagnostic-fixture category=page')
    expect(report).toContain('category=line page=1 block=7')
    expect(report).toContain('expected=20 actual=20.02')
  })

  it('keeps geometry tolerance explicit and below meaningful placement changes', () => {
    expect(compareDiagnosticParity({ pages: [{ top: 10 }] }, { pages: [{ top: 10.009 }] })).toEqual(
      [],
    )
    expect(
      compareDiagnosticParity({ pages: [{ top: 10 }] }, { pages: [{ top: 10.02 }] }),
    ).toHaveLength(1)
  })

  it('observes rendered page, gap, header/footer, float, caret, and multi-rect selection geometry', () => {
    const rect = (left: number, top: number, width: number, height: number) =>
      ({
        left,
        top,
        width,
        height,
        right: left + width,
        bottom: top + height,
      }) as DOMRect
    const setRect = (el: HTMLElement, value: DOMRect) => {
      Object.defineProperty(el, 'getBoundingClientRect', {
        configurable: true,
        value: () => value,
      })
    }

    const root = document.createElement('div')
    root.className = 'page-wrap'
    const page = document.createElement('div')
    page.className = 'doc-page'
    page.style.setProperty('--page-h', '100px')
    const pm = document.createElement('div')
    pm.className = 'ProseMirror'
    const gap = document.createElement('div')
    gap.className = 'page-gap'
    gap.style.height = '50px'
    gap.style.setProperty('--gap-mb', '10px')
    gap.style.setProperty('--gap-mt', '12px')
    const header = document.createElement('div')
    header.className = 'page-hf page-hf-header page-gap-hf'
    gap.append(header)
    const float = document.createElement('div')
    float.className = 'doc-textbox'
    float.dataset.pageFloatDy = '4'
    const floatHost = document.createElement('div')
    floatHost.className = 'doc-protected-floating'
    floatHost.append(float)
    pm.append(floatHost, gap)
    page.append(pm)
    root.append(page)
    document.body.append(root)

    setRect(root, rect(100, 200, 200, 500))
    setRect(page, rect(100, 200, 200, 300))
    setRect(pm, rect(110, 210, 180, 200))
    setRect(gap, rect(100, 290, 200, 50))
    setRect(header, rect(110, 318, 180, 12))
    setRect(float, rect(120, 230, 40, 20))

    const selectionRects = [rect(115, 225, 20, 14), rect(115, 345, 30, 14)]
    const originalClientRects = Range.prototype.getClientRects
    Object.defineProperty(Range.prototype, 'getClientRects', {
      configurable: true,
      value: () => selectionRects,
    })
    try {
      const view = {
        dom: pm,
        state: { selection: { anchor: 2, head: 5, from: 2, to: 5 } },
        coordsAtPos: () => rect(116, 225, 1, 14),
        posAtCoords: () => ({ pos: 2 }),
        domAtPos: () => ({ node: pm, offset: 0 }),
      }
      const parityInput = {
        blocks: [
          { top: 0, height: 90 },
          { top: 90, height: 90 },
        ],
        sectionGeoms: [{ contentHeight: 90, forceBreak: false }],
        totalHeight: 180,
        zoomFactor: 1,
      }
      const v1Slices = renderPresentation('v1', parityInput)
      const v2Slices = renderPresentation('v2', parityInput)
      expect(v1Slices).toEqual(v2Slices)
      const source = {
        root,
        flowRoot: pm,
        slices: [
          { start: 0, end: 90, section: 0 },
          { start: 90, end: 180, section: 0 },
        ],
        zoomFactor: 1,
        editorView: view,
        floatBoxes: [{ el: float, top: 10, height: 20 }],
        blockOf: () => 7,
      } as const
      const observed = capturePostRenderDiagnostics(source)
      const caretObserved = capturePostRenderDiagnostics({
        ...source,
        editorView: {
          ...view,
          state: { selection: { anchor: 1, head: 1, from: 1, to: 1 } },
        },
      })
      const parity = compareDiagnosticParity(
        { postRender: observed },
        { postRender: capturePostRenderDiagnostics(source) },
        { fixture: 'post-render-dom' },
      )

      expect(parity).toEqual([])
      expect(observed.pages[0].pageRect).toMatchObject({ top: 0, width: 200, height: 100 })
      expect(observed.pages[0]).toMatchObject({ page: 1, pageIndex: 0 })
      expect(observed.pages[1].pageRect).toMatchObject({ top: 128, width: 200, height: 100 })
      expect(observed.pages[1]).toMatchObject({ page: 2, pageIndex: 1 })
      expect(observed.pageGaps[0]).toMatchObject({
        page: 2,
        pageIndex: 1,
        sizePx: 50,
        bandRect: { top: 100, height: 28 },
      })
      expect(observed.headerFooters[0]).toMatchObject({ page: 2, pageIndex: 1, kind: 'header' })
      expect(observed.floats[0]).toMatchObject({ page: 1, block: 7, domShiftY: 4 })
      expect(caretObserved.caret).toMatchObject({ position: 1, page: 1, pageIndex: 0 })
      expect(observed.selection).toMatchObject({
        pmRange: { from: 2, to: 5 },
        pages: [1, 2],
        pageIndexes: [0, 1],
      })
      expect(observed.coordinateSpaces).toMatchObject({
        pageIndex: 'zero-based',
        pageNumber: 'one-based-legacy-diagnostic',
      })
      expect(observed.selection?.rects).toHaveLength(2)
    } finally {
      Object.defineProperty(Range.prototype, 'getClientRects', {
        configurable: true,
        value: originalClientRects,
      })
      root.remove()
    }
  })
})
