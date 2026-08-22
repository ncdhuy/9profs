import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { blocksToPmDoc, pmDocToSavePlan, type PmNode } from '../src/renderer/editor/convert'
import { isDocDirty, type DocDirtyState } from '../src/renderer/doc-dirty'
import {
  renderPresentation,
  renderPresentationSnapshot,
  renderPresentationV1,
  resolvePresentationRenderer,
  type PresentationRenderer,
} from '../src/renderer/presentation-v2'
import { parseDocx, readSections, saveDocx, type Block } from '@genoffice/docx-engine'
import type { BlockBox, FloatBox, PageSlice } from '../src/renderer/pagination'

const __dirname = dirname(fileURLToPath(import.meta.url))
const SIMPLE_FIXTURE = join(__dirname, 'pagination-corpus/docx/fixture-simple.docx')

function presentationInput() {
  const floats: FloatBox[] = [{ el: {} as HTMLElement, top: 900, height: 120 }]
  const blocks: BlockBox[] = [
    {
      top: 0,
      height: 640,
      docxIndex: 3,
      lineBoxes: [
        { offsetInBlock: 0, height: 160 },
        { offsetInBlock: 160, height: 160 },
        { offsetInBlock: 320, height: 160 },
        { offsetInBlock: 480, height: 160 },
      ],
    },
    {
      top: 640,
      height: 700,
      docxIndex: 7,
      tableRows: [{ height: 220, isHeader: true }, { height: 240 }, { height: 240 }],
      fixedWidthPx: 280,
    },
  ]
  return {
    blocks,
    sectionGeoms: [{ contentHeight: 800, forceBreak: false, cols: 2, colWidths: [300, 300] }],
    totalHeight: 1340,
    zoomFactor: 1,
    floats,
  }
}

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

function persistenceBlock(): Block {
  return {
    id: 'seam-0',
    type: 'paragraph',
    docxIndex: 0,
    originalXml: '<w:p><w:r><w:t>seam</w:t></w:r></w:p>',
    runs: [{ text: 'seam' }],
  }
}

function pageGapBoundaryInputs(snapshot: ReturnType<typeof renderPresentationSnapshot>) {
  return snapshot.pages.slice(1).map((page, index) => ({
    previousStart: snapshot.pages[index].start,
    previousEnd: snapshot.pages[index].end,
    start: page.start,
    section: page.section,
  }))
}

describe('DOCX presentation-v2 seam', () => {
  it('defaults to V1 and accepts deterministic internal V1/V2 selection', () => {
    expect(resolvePresentationRenderer()).toBe('v1')
    expect(resolvePresentationRenderer('v1')).toBe('v1')
    expect(resolvePresentationRenderer('v2')).toBe('v2')
    expect(resolvePresentationRenderer('unknown')).toBe('v1')

    const globalState = globalThis as typeof globalThis & {
      __9profsDocsPresentationRenderer?: unknown
    }
    const previous = globalState.__9profsDocsPresentationRenderer
    try {
      globalState.__9profsDocsPresentationRenderer = 'v2'
      expect(resolvePresentationRenderer()).toBe('v2')
    } finally {
      if (previous === undefined) delete globalState.__9profsDocsPresentationRenderer
      else globalState.__9profsDocsPresentationRenderer = previous
    }
  })

  it('keeps representative line/table pagination output identical', () => {
    const render = (renderer: PresentationRenderer): PageSlice[] =>
      renderPresentation(renderer, presentationInput())

    expect(render('v2')).toEqual(render('v1'))
  })

  it('exposes equivalent V1/V2 layout snapshots without changing existing identities', () => {
    const input = presentationInput()
    const v1 = renderPresentationSnapshot('v1', input)
    const v2 = renderPresentationSnapshot('v2', input)

    expect(v1.pages).toEqual(renderPresentationV1(input))
    expect(renderPresentation('v1', input)).toEqual(v1.pages)
    expect(v2.pages).toEqual(v1.pages)
    expect(v1.blocks).toBe(input.blocks)
    expect(v1.sectionGeoms).toBe(input.sectionGeoms)
    expect(v1.floats).toBe(input.floats)
    expect(v2.floats).toBe(input.floats)
    expect(v1.blocks.map((block) => block.docxIndex)).toEqual([3, 7])
    expect(v1.pages.every((page) => v1.sectionGeoms[page.section] === input.sectionGeoms[page.section])).toBe(
      true,
    )
  })

  it('keeps page-gap boundary inputs equivalent through the shared snapshot', () => {
    const input = presentationInput()
    const v1 = renderPresentationSnapshot('v1', input)
    const v2 = renderPresentationSnapshot('v2', input)

    expect(pageGapBoundaryInputs(v1)).toHaveLength(v1.pages.length - 1)
    expect(pageGapBoundaryInputs(v2)).toEqual(pageGapBoundaryInputs(v1))
    expect(pageGapBoundaryInputs(v2).map((boundary) => boundary.start)).toEqual(
      pageGapBoundaryInputs(v1).map((boundary) => boundary.start),
    )
    expect(pageGapBoundaryInputs(v1).every(({ section }) => input.sectionGeoms[section])).toBe(true)
  })

  it('does not change PM JSON, dirty state, or save plan', () => {
    const block = persistenceBlock()
    const snapshot = (renderer: PresentationRenderer) => {
      renderPresentation(renderer, presentationInput())
      const pmDoc = blocksToPmDoc([block])
      return {
        pmJson: pmDoc,
        dirty: [
          isDocDirty(cleanDirtyState()),
          isDocDirty({ ...cleanDirtyState(), dirtyRef: { current: true } }),
          isDocDirty({ ...cleanDirtyState(), commentsDirty: true }),
        ],
        savePlan: pmDocToSavePlan(pmDoc, [block]),
      }
    }

    const v1 = snapshot('v1')
    const v2 = snapshot('v2')
    expect(v2).toEqual(v1)
    expect(v1.dirty).toEqual([false, true, true])
  })

  it('keeps save/reopen behavior identical for a DOCX fixture', async () => {
    const sourceBytes = new Uint8Array(readFileSync(SIMPLE_FIXTURE))
    const run = async (renderer: PresentationRenderer) => {
      const slices = renderPresentation(renderer, presentationInput())
      const parsed = await parseDocx(sourceBytes)
      const pmDoc = blocksToPmDoc(parsed.blocks, readSections(parsed))
      const savePlan = pmDocToSavePlan(pmDoc, parsed.blocks)
      const savedBytes = await saveDocx(parsed, savePlan.saveBlocks)
      const reopened = await parseDocx(savedBytes)
      return {
        slices,
        pmJson: pmDoc,
        dirty: isDocDirty(cleanDirtyState()),
        savePlan,
        savedBytes: Array.from(savedBytes),
        reopenedPmJson: blocksToPmDoc(reopened.blocks, readSections(reopened)),
      }
    }

    const [v1, v2] = await Promise.all([run('v1'), run('v2')])
    expect(v2).toEqual(v1)
    expect(v1.savedBytes).toEqual(Array.from(sourceBytes))
  })
})
