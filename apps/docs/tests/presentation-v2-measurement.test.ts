import { describe, expect, it, vi } from 'vitest'
import {
  bumpLineSampleFontEpoch,
  fillLineBoxes,
  paginationMeasurementCandidates,
  type BlockBox,
  type BlockMetaOf,
  type PageSlice,
  type SectionGeom,
} from '../src/renderer/pagination'
import { refinePresentationMeasurementsV2 } from '../src/renderer/presentation-v2/measurement'
import {
  createPresentationMeasurementContextV2,
  shouldInvalidateMeasurementV2,
} from '../src/renderer/presentation-v2/measurement-context'

const geometry = (top: number, height: number, width = 240): DOMRect =>
  ({ top, bottom: top + height, height, width }) as DOMRect

const sectionGeoms: SectionGeom[] = [{ contentHeight: 80, forceBreak: false }]
const pages: PageSlice[] = [{ start: 0, end: 80, section: 0 }]

function textBlock(width = 240): BlockBox {
  const el = document.createElement('p')
  el.textContent = 'wrapped paragraph'
  el.getBoundingClientRect = () => geometry(100, 120, width)
  document.body.appendChild(el)
  return { el, top: 0, height: 120 }
}

function tableBlock(reads: { rows: number }): BlockBox {
  const table = document.createElement('table')
  const body = document.createElement('tbody')
  const first = document.createElement('tr')
  const second = document.createElement('tr')
  first.appendChild(document.createElement('td'))
  second.appendChild(document.createElement('td'))
  body.append(first, second)
  table.appendChild(body)
  table.getBoundingClientRect = () => geometry(0, 100)
  first.getBoundingClientRect = () => {
    reads.rows++
    return geometry(0, 40)
  }
  second.getBoundingClientRect = () => {
    reads.rows++
    return geometry(40, 60)
  }
  document.body.appendChild(table)
  return { el: table, top: 0, height: 100, docxIndex: 1 }
}

function lineRectsMock() {
  const range = Range.prototype as Range & { getClientRects?: () => DOMRectList }
  const previous = range.getClientRects
  const getClientRects = vi.fn(
    () => [geometry(100, 10), geometry(120, 10)] as unknown as DOMRectList,
  )
  Object.defineProperty(range, 'getClientRects', { configurable: true, value: getClientRects })
  return {
    getClientRects,
    restore: () => {
      if (previous)
        Object.defineProperty(range, 'getClientRects', { configurable: true, value: previous })
      else delete (range as { getClientRects?: () => DOMRectList }).getClientRects
    },
  }
}

describe('Presentation V2 measurement refinement', () => {
  it('matches V1 lineBoxes and changed output using shared DOM sampling', () => {
    const rects = lineRectsMock()
    try {
      const v1 = textBlock()
      const v2 = textBlock()
      const metaOf = () => undefined

      const v1Changed = fillLineBoxes([v1], sectionGeoms, 1, pages, metaOf)
      const v2Changed = refinePresentationMeasurementsV2({
        blocks: [v2],
        sectionGeoms,
        pages,
        zoomFactor: 1,
        metaOf,
      })

      expect(v2.lineBoxes).toEqual(v1.lineBoxes)
      expect(v2Changed).toBe(v1Changed)
      expect(rects.getClientRects).toHaveBeenCalled()
    } finally {
      rects.restore()
      document.body.replaceChildren()
    }
  })

  it('matches V1 tableRows, flags, and changed output', () => {
    const v1Reads = { rows: 0 }
    const v2Reads = { rows: 0 }
    const v1 = tableBlock(v1Reads)
    const v2 = tableBlock(v2Reads)
    const metaOf: BlockMetaOf = () => ({
      tableRowFlags: [
        { isHeader: true, cantSplit: true },
        { isHeader: false, cantSplit: true },
      ],
    })

    const v1Changed = fillLineBoxes([v1], sectionGeoms, 1, pages, metaOf)
    const v2Changed = refinePresentationMeasurementsV2({
      blocks: [v2],
      sectionGeoms,
      pages,
      zoomFactor: 1,
      metaOf,
    })

    expect(v2.tableRows).toEqual(v1.tableRows)
    expect(v2Changed).toBe(v1Changed)
    expect(v1Reads.rows).toBeGreaterThan(0)
    expect(v2Reads.rows).toBeGreaterThan(0)
    document.body.replaceChildren()
  })

  it('preserves page, column, oversized, and keepNext candidate semantics', () => {
    const make = (top: number, height: number, keepNext = false): BlockBox => {
      const el = document.createElement('p')
      el.getBoundingClientRect = () => geometry(top, height)
      document.body.appendChild(el)
      return { el, top, height, keepNext }
    }
    const regular = make(0, 20)
    const columnCrossing = make(40, 20)
    const pageTop = make(80, 20)
    const oversized = make(100, 100)
    const keepAnchor = make(220, 20, true)
    const keepFollower = make(240, 20)
    const protectedBlock = make(300, 100)
    protectedBlock.el!.classList.add('doc-protected-textboxes')
    const blocks = [
      regular,
      columnCrossing,
      pageTop,
      oversized,
      keepAnchor,
      keepFollower,
      protectedBlock,
    ]
    const candidateIndexes = paginationMeasurementCandidates(blocks, sectionGeoms, [
      {
        start: 0,
        end: 160,
        section: 0,
        regions: [
          {
            top: 0,
            height: 80,
            section: 0,
            columns: [
              { start: 0, end: 50 },
              { start: 50, end: 80 },
            ],
          },
        ],
      },
      { start: 80, end: 160, section: 0 },
    ]).map(({ block }) => blocks.indexOf(block))

    expect(candidateIndexes).toEqual([1, 2, 3, 5])
    document.body.replaceChildren()
  })

  it('keeps shared cache reuse and invalidation semantics', () => {
    const rects = lineRectsMock()
    try {
      const block = textBlock()
      const measurementContext = createPresentationMeasurementContextV2(1)
      const run = () => {
        block.lineBoxes = undefined
        return refinePresentationMeasurementsV2({
          blocks: [block],
          sectionGeoms,
          pages,
          zoomFactor: 1,
          measurementContext,
        })
      }

      run()
      const initialSamples = rects.getClientRects.mock.calls.length
      run()
      expect(rects.getClientRects.mock.calls.length).toBe(initialSamples)

      block.el!.textContent = 'changed text'
      run()
      const afterTextChange = rects.getClientRects.mock.calls.length
      expect(afterTextChange).toBeGreaterThan(initialSamples)

      let width = 240
      block.el!.getBoundingClientRect = () => geometry(100, 120, width)
      width = 320
      run()
      const afterWidthChange = rects.getClientRects.mock.calls.length
      expect(afterWidthChange).toBeGreaterThan(afterTextChange)

      bumpLineSampleFontEpoch()
      run()
      expect(rects.getClientRects.mock.calls.length).toBeGreaterThan(afterWidthChange)
    } finally {
      rects.restore()
      document.body.replaceChildren()
    }
  })

  it('owns one coherent V2 measurement context and refreshes after environment changes', () => {
    const context = createPresentationMeasurementContextV2(1)
    expect(Object.isFrozen(context)).toBe(true)
    expect(shouldInvalidateMeasurementV2(context, 1)).toBe(false)

    bumpLineSampleFontEpoch()
    expect(shouldInvalidateMeasurementV2(context, 1)).toBe(true)

    const refreshed = createPresentationMeasurementContextV2(1)
    expect(shouldInvalidateMeasurementV2(refreshed, 1)).toBe(false)
    expect(shouldInvalidateMeasurementV2(refreshed, 1.25)).toBe(true)
  })

  it('remeasures when V2 zoom scale changes even if rendered width is unchanged', () => {
    const rects = lineRectsMock()
    try {
      const block = textBlock()
      const run = (zoomFactor: number) => {
        block.lineBoxes = undefined
        return refinePresentationMeasurementsV2({
          blocks: [block],
          sectionGeoms,
          pages,
          zoomFactor,
          measurementContext: createPresentationMeasurementContextV2(zoomFactor),
        })
      }

      run(1)
      const initialSamples = rects.getClientRects.mock.calls.length
      const firstLines = block.lineBoxes
      run(2)

      expect(rects.getClientRects.mock.calls.length).toBeGreaterThan(initialSamples)
      expect(block.lineBoxes).not.toEqual(firstLines)
    } finally {
      rects.restore()
      document.body.replaceChildren()
    }
  })

  it('reuses shared samples across stable V2 runs and converges on the next pass', () => {
    const rects = lineRectsMock()
    try {
      const block = textBlock()
      const measurementContext = createPresentationMeasurementContextV2(1)
      const refine = () =>
        refinePresentationMeasurementsV2({
          blocks: [block],
          sectionGeoms,
          pages,
          zoomFactor: 1,
          measurementContext,
        })

      expect(refine()).toBe(true)
      const initialSamples = rects.getClientRects.mock.calls.length
      expect(refine()).toBe(false)

      block.lineBoxes = undefined
      expect(refine()).toBe(true)
      expect(rects.getClientRects.mock.calls.length).toBe(initialSamples)
    } finally {
      rects.restore()
      document.body.replaceChildren()
    }
  })

  it('remeasures table rows when row geometry changes without text or table width changes', () => {
    const reads = { rows: 0 }
    const table = document.createElement('table')
    const body = document.createElement('tbody')
    const first = document.createElement('tr')
    const second = document.createElement('tr')
    first.appendChild(document.createElement('td'))
    second.appendChild(document.createElement('td'))
    body.append(first, second)
    table.appendChild(body)
    let secondHeight = 60
    table.getBoundingClientRect = () => geometry(0, 100, 240)
    first.getBoundingClientRect = () => geometry(0, 40, 240)
    second.getBoundingClientRect = () => {
      reads.rows++
      return geometry(40, secondHeight, 240)
    }
    document.body.appendChild(table)
    const block: BlockBox = { el: table, top: 0, height: 100 }

    try {
      const run = () => {
        block.tableRows = undefined
        return refinePresentationMeasurementsV2({
          blocks: [block],
          sectionGeoms,
          pages,
          zoomFactor: 1,
        })
      }

      run()
      const firstSampleReads = reads.rows
      secondHeight = 80
      run()

      expect(reads.rows).toBeGreaterThan(firstSampleReads)
    } finally {
      document.body.replaceChildren()
    }
  })
})
