import { Schema } from '@tiptap/pm/model'
import { EditorState } from '@tiptap/pm/state'
import { describe, expect, it, vi } from 'vitest'
import {
  getLineSampleFontEpoch,
  type BlockBox,
  type PageSlice,
  type SectionGeom,
} from '../src/renderer/pagination'
import {
  createFullPresentationInvalidationHint,
  mergePresentationInvalidationHints,
  presentationScheduleDelayMs,
  presentationInvalidationHintFromTransaction,
} from '../src/renderer/presentation-v2/measurement-invalidation'
import { createPresentationRefinementWindowV2 } from '../src/renderer/presentation-v2/measurement'
import { paginatePresentationV2 } from '../src/renderer/presentation-v2/page-slicer'
import {
  createPresentationSchedulerRecorder,
  createPresentationV2PerformanceRecorder,
} from '../src/renderer/presentation-v2/performance'

const textSchema = new Schema({
  nodes: {
    doc: { content: 'block+' },
    paragraph: { content: 'inline*', group: 'block' },
    text: { group: 'inline' },
  },
})

const geometry = (top: number, height: number, width = 240): DOMRect =>
  ({ top, bottom: top + height, height, width }) as DOMRect

function layoutBlocks(count: number): BlockBox[] {
  return Array.from({ length: count }, (_, index) => {
    const el = document.createElement('p')
    el.textContent = `long paragraph ${index}`
    el.getBoundingClientRect = () => geometry(index * 60, 60)
    document.body.appendChild(el)
    return { el, top: index * 60, height: 60 }
  })
}

function cloneLayoutBlocks(blocks: BlockBox[]): BlockBox[] {
  return blocks.map(({ el, top, height }) => ({ el, top, height }))
}

function layoutSignature(blocks: BlockBox[], pages: PageSlice[]) {
  return JSON.stringify({
    pages,
    blocks: blocks.map((block) => ({
      top: block.top,
      height: block.height,
      lines: block.lineBoxes,
      rows: block.tableRows,
    })),
  })
}

describe('Presentation V2 dirty-range measurement pruning', () => {
  it('uses bounded fast-local scheduling and conservative structural timing', () => {
    expect(presentationScheduleDelayMs('FAST_LOCAL', 100)).toBe(50)
    expect(presentationScheduleDelayMs('FAST_LOCAL', 340, 100)).toBe(10)
    expect(presentationScheduleDelayMs('FAST_LOCAL', 400, 100)).toBe(0)
    expect(presentationScheduleDelayMs('CONSERVATIVE', 100, 0)).toBe(300)

    vi.stubGlobal('__9profsDocsPresentationLocalDelayMs', 50)
    expect(presentationScheduleDelayMs('FAST_LOCAL', 100)).toBe(50)
    vi.unstubAllGlobals()
  })

  it('records scheduler acceptance, timer wait, execution, settle, and stale callbacks', () => {
    const recorder = createPresentationSchedulerRecorder()
    recorder.onTransaction('FAST_LOCAL', false, 10)
    recorder.onSchedulerAccepted('FAST_LOCAL', 11)
    recorder.onTimerScheduled(false)
    const runToken = recorder.onLayoutStart(true, 111)
    recorder.onLayoutEnd(runToken, 125)
    recorder.onSettled(runToken, 141)
    recorder.onStaleTimerCallback()

    expect(recorder.snapshot()).toMatchObject({
      transactionsReceived: 1,
      fastLocalTransactions: 1,
      scheduledLayouts: 1,
      layoutRuns: 1,
      schedulerWaitMs: 100,
      layoutExecutionMs: 14,
      settleMs: 16,
      transactionToSchedulerAcceptedMs: 1,
      staleTimerCallbacks: 1,
      lastScheduleClass: 'FAST_LOCAL',
    })
  })

  it('classifies ordinary text replacement and falls back for structural changes', () => {
    const doc = textSchema.node('doc', null, [
      textSchema.node('paragraph', null, textSchema.text('ordinary body text')),
    ])
    const state = EditorState.create({ schema: textSchema, doc })
    const replacement = state.tr.insertText('x', 2, 3)
    const hint = presentationInvalidationHintFromTransaction(replacement, 4, 7)

    expect(hint).toMatchObject({
      kind: 'local',
      topLevelIndex: 0,
      layoutEpoch: 4,
      fontEpoch: 7,
    })
    expect(
      presentationInvalidationHintFromTransaction(
        state.tr.insertText(' inserted text that can change line count', 2),
        4,
        7,
      ),
    ).toMatchObject({ kind: 'local', topLevelIndex: 0 })
    expect(presentationInvalidationHintFromTransaction(state.tr.delete(2, 8), 4, 7)).toMatchObject({
      kind: 'local',
      topLevelIndex: 0,
    })

    const structural = state.tr.replaceWith(0, state.doc.content.size, doc)
    expect(presentationInvalidationHintFromTransaction(structural, 4, 7)).toMatchObject({
      kind: 'full',
      reason: 'unknown-transaction',
    })
  })

  it('merges a debounce window by preserving the earliest local edit', () => {
    const first = presentationInvalidationHintFromTransaction(
      EditorState.create({
        schema: textSchema,
        doc: textSchema.node('doc', null, [
          textSchema.node('paragraph', null, textSchema.text('first')),
          textSchema.node('paragraph', null, textSchema.text('second')),
        ]),
      }).tr.insertText('x', 9),
      1,
      2,
    )
    const later = createFullPresentationInvalidationHint(1, 2, 'structural-transaction')
    expect(mergePresentationInvalidationHints(null, first)).toBe(first)
    expect(mergePresentationInvalidationHints(first, later)).toMatchObject({
      kind: 'full',
      reason: 'structural-transaction',
    })
  })

  it('chooses a preceding page/column boundary and includes keepNext anchors', () => {
    const blocks: BlockBox[] = [
      { top: 0, height: 30, keepNext: true },
      { top: 30, height: 30 },
      { top: 60, height: 50 },
      { top: 110, height: 50 },
    ]
    const pages: PageSlice[] = [
      { start: 0, end: 100, section: 0 },
      { start: 100, end: 200, section: 0 },
    ]
    const window = createPresentationRefinementWindowV2(blocks, pages, {
      kind: 'local',
      blockIndex: 3,
      topLevelIndex: 3,
      layoutEpoch: 1,
      fontEpoch: getLineSampleFontEpoch(),
      reason: 'local-text',
    })

    expect(window).toEqual({ fromBlockIndex: 2, restartPageIndex: 1 })
  })

  it('falls back to full V2 refinement when the font environment is not stable', () => {
    const range = Range.prototype as Range & { getClientRects?: () => DOMRectList }
    const previous = range.getClientRects
    Object.defineProperty(range, 'getClientRects', {
      configurable: true,
      value: () => [geometry(0, 10)] as unknown as DOMRectList,
    })
    const blocks = layoutBlocks(4)
    const recorder = createPresentationV2PerformanceRecorder()
    try {
      paginatePresentationV2({
        blocks,
        sectionGeoms: [{ contentHeight: 120, forceBreak: false }],
        totalHeight: 480,
        zoomFactor: 1,
        invalidationHint: {
          kind: 'local',
          topLevelIndex: 3,
          blockIndex: 3,
          layoutEpoch: 1,
          fontEpoch: getLineSampleFontEpoch() + 1,
          reason: 'local-text',
        },
        performance: recorder.sink,
      })

      const snapshot = recorder.snapshot()
      expect(snapshot.fullRefinementFallbackReasons).toContain('font-epoch')
      expect(snapshot.measurementCandidatesSkipped).toBe(0)
      expect(snapshot.measurementCandidatesVisited).toBeGreaterThan(0)
    } finally {
      if (previous)
        Object.defineProperty(range, 'getClientRects', { configurable: true, value: previous })
      else delete (range as { getClientRects?: () => DOMRectList }).getClientRects
      document.body.replaceChildren()
    }
  })

  it('matches forced-full V2 output while skipping a stable cached prefix', () => {
    const range = Range.prototype as Range & { getClientRects?: () => DOMRectList }
    const previous = range.getClientRects
    const getClientRects = vi.fn(function (this: Range) {
      const parent = this.commonAncestorContainer.parentElement
      const top = parent?.getBoundingClientRect().top ?? 0
      return [geometry(top, 10), geometry(top + 20, 10)] as unknown as DOMRectList
    })
    Object.defineProperty(range, 'getClientRects', { configurable: true, value: getClientRects })
    try {
      const sectionGeoms: SectionGeom[] = [{ contentHeight: 80, forceBreak: false }]
      const seed = layoutBlocks(8)
      paginatePresentationV2({
        blocks: seed,
        sectionGeoms,
        totalHeight: 480,
        zoomFactor: 1,
        forceFullRefinement: true,
      })

      const oracleBlocks = cloneLayoutBlocks(seed)
      const oraclePages = paginatePresentationV2({
        blocks: oracleBlocks,
        sectionGeoms,
        totalHeight: 480,
        zoomFactor: 1,
        forceFullRefinement: true,
      })
      const optimizedBlocks = cloneLayoutBlocks(seed)
      const recorder = createPresentationV2PerformanceRecorder()
      const optimizedPages = paginatePresentationV2({
        blocks: optimizedBlocks,
        sectionGeoms,
        totalHeight: 480,
        zoomFactor: 1,
        invalidationHint: {
          kind: 'local',
          topLevelIndex: 7,
          blockIndex: 7,
          layoutEpoch: 1,
          fontEpoch: getLineSampleFontEpoch(),
          reason: 'local-text',
        },
        performance: recorder.sink,
      })

      expect(layoutSignature(optimizedBlocks, optimizedPages)).toBe(
        layoutSignature(oracleBlocks, oraclePages),
      )
      const snapshot = recorder.snapshot()
      expect(snapshot.measurementCandidatesSkipped).toBeGreaterThan(0)
      expect(snapshot.measurementCandidatesVisited).toBeLessThan(
        snapshot.measurementCandidatesTotal,
      )
      expect(snapshot.measurementCacheRestores).toBeGreaterThan(0)
    } finally {
      if (previous)
        Object.defineProperty(range, 'getClientRects', { configurable: true, value: previous })
      else delete (range as { getClientRects?: () => DOMRectList }).getClientRects
      document.body.replaceChildren()
    }
  })
})
