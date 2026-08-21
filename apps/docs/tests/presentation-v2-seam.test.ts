import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { blocksToPmDoc, pmDocToSavePlan, type PmNode } from '../src/renderer/editor/convert'
import { isDocDirty, type DocDirtyState } from '../src/renderer/doc-dirty'
import {
  renderPresentation,
  resolvePresentationRenderer,
  type PresentationRenderer,
} from '../src/renderer/presentation-v2'
import { parseDocx, readSections, saveDocx, type Block } from '@genoffice/docx-engine'
import type { BlockBox, PageSlice } from '../src/renderer/pagination'

const __dirname = dirname(fileURLToPath(import.meta.url))
const SIMPLE_FIXTURE = join(__dirname, 'pagination-corpus/docx/fixture-simple.docx')

function presentationInput() {
  const blocks: BlockBox[] = [
    {
      top: 0,
      height: 640,
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
      tableRows: [{ height: 220, isHeader: true }, { height: 240 }, { height: 240 }],
      fixedWidthPx: 280,
    },
  ]
  return {
    blocks,
    sectionGeoms: [{ contentHeight: 800, forceBreak: false, cols: 2, colWidths: [300, 300] }],
    totalHeight: 1340,
    zoomFactor: 1,
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
