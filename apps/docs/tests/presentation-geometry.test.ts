import { afterEach, describe, expect, it } from 'vitest'
import {
  createPresentationGeometry,
  snapshotPresentationGeometry,
  type GeometryPoint,
} from '../src/renderer/presentation-v2'
import { compareDiagnosticParity } from '../src/renderer/presentation-v2/diagnostics'

const rect = (left: number, top: number, width: number, height: number) =>
  ({ left, top, width, height, right: left + width, bottom: top + height }) as DOMRect

describe('neutral Presentation Geometry API', () => {
  let originalRangeRects: typeof Range.prototype.getClientRects | undefined

  afterEach(() => {
    if (originalRangeRects) {
      Object.defineProperty(Range.prototype, 'getClientRects', {
        configurable: true,
        value: originalRangeRects,
      })
      originalRangeRects = undefined
    }
    document.body.replaceChildren()
  })

  function makeGeometry() {
    const root = document.createElement('div')
    root.className = 'page-wrap'
    const page = document.createElement('div')
    page.className = 'doc-page'
    page.style.setProperty('--page-h', '100px')
    const flowRoot = document.createElement('div')
    flowRoot.className = 'ProseMirror'
    const text = document.createTextNode('geometry')
    flowRoot.append(text)
    const gap = document.createElement('div')
    gap.className = 'page-gap'
    gap.style.setProperty('--gap-mt', '10px')
    gap.style.setProperty('--gap-mb', '5px')
    root.append(page, flowRoot, gap)
    document.body.append(root)

    const rectangles = new Map<number, DOMRect>([
      [4, rect(20, 40, 1, 14)],
      [15, rect(20, 152, 1, 14)],
    ])
    Object.defineProperty(root, 'getBoundingClientRect', {
      configurable: true,
      value: () => rect(0, 0, 300, 300),
    })
    Object.defineProperty(page, 'getBoundingClientRect', {
      configurable: true,
      value: () => rect(10, 20, 200, 100),
    })
    Object.defineProperty(flowRoot, 'getBoundingClientRect', {
      configurable: true,
      value: () => rect(10, 20, 200, 220),
    })
    Object.defineProperty(gap, 'getBoundingClientRect', {
      configurable: true,
      value: () => rect(10, 120, 200, 40),
    })
    originalRangeRects = Range.prototype.getClientRects
    Object.defineProperty(Range.prototype, 'getClientRects', {
      configurable: true,
      value: () => [rect(20, 40, 30, 14), rect(20, 152, 20, 14)],
    })

    const view = {
      dom: flowRoot,
      coordsAtPos: (position: number) => rectangles.get(position) ?? rect(20, 40, 1, 14),
      posAtCoords: ({ top }: { left: number; top: number }) => ({ pos: top >= 150 ? 15 : 4 }),
      domAtPos: () => ({ node: text, offset: 0 }),
    }
    const geometry = createPresentationGeometry({
      root,
      flowRoot,
      slices: [
        {
          start: 0,
          end: 100,
          section: 0,
          regions: [
            {
              top: 0,
              height: 100,
              section: 0,
              columns: [
                { start: 0, end: 50 },
                { start: 50, end: 100 },
              ],
            },
          ],
        },
        {
          start: 100,
          end: 200,
          section: 1,
          regions: [
            {
              top: 0,
              height: 100,
              section: 1,
              columns: [
                { start: 100, end: 150 },
                { start: 150, end: 200 },
              ],
            },
          ],
        },
      ],
      blocks: [
        { top: 0, height: 80, lineBoxes: [{ offsetInBlock: 0, height: 24 }] },
        { top: 100, height: 80, docxIndex: 8, lineBoxes: [{ offsetInBlock: 0, height: 24 }] },
      ],
      editorView: view,
      zoomFactor: 1,
    })
    return { geometry, view }
  }

  it('normalizes page, flow, document, and page-local geometry', () => {
    const { geometry } = makeGeometry()
    const first = geometry.positionToGeometry(4)
    const second = geometry.locatePosition(15)

    expect(geometry.coordinateSpaces).toEqual({
      viewport: 'browser-viewport-css-px',
      document: 'page-wrap-relative-css-px',
      pageLocal: 'page-relative-css-px',
      flow: 'gapless-layout-px-at-100-percent',
      zoomFactor: 1,
    })
    expect(first).toMatchObject({
      status: 'resolved',
      pageIndex: 0,
      sectionIndex: 0,
      flowRect: { space: 'flow', top: 20 },
      documentRect: { space: 'document', left: 20, top: 40 },
      pageLocalRect: { space: 'page-local', left: 10, top: 20 },
      line: { index: 0, flowTop: 0 },
      columnIndex: 0,
      columnCount: 2,
      block: { index: 0 },
    })
    expect(second).toMatchObject({
      pageIndex: 1,
      sectionIndex: 1,
      documentRect: { space: 'document', top: 152 },
      pageLocalRect: { space: 'page-local', top: 2 },
      block: { index: 1, docxIndex: 8 },
    })

    expect(geometry.pageGeometry(1)).toMatchObject({
      pageIndex: 1,
      sectionIndex: 1,
      pageWidth: 200,
      pageHeight: 100,
      physicalOrigin: { space: 'document', x: 10, y: 150 },
      pageLocalOrigin: { space: 'page-local', x: 0, y: 0, pageIndex: 1 },
      flowOffset: { space: 'flow', x: 0, y: 100 },
      gapBefore: {
        boundary: { fromPageIndex: 0, toPageIndex: 1 },
        documentRect: { space: 'document', top: 120 },
      },
    })
  })

  it('maps explicit coordinate-space points without leaking viewport coordinates', () => {
    const { geometry } = makeGeometry()
    const points: GeometryPoint[] = [
      { space: 'viewport', x: 20, y: 40 },
      { space: 'document', x: 20, y: 40 },
      { space: 'flow', x: 10, y: 20 },
      { space: 'page-local', pageIndex: 1, x: 10, y: 2 },
    ]
    for (const point of points)
      expect(geometry.pointToPosition(point)).toMatchObject({
        status: 'resolved',
        pmPosition: point.space === 'page-local' ? 15 : 4,
      })
    expect(geometry.pointToPosition({ space: 'page-local', x: 1, y: 1 })).toEqual({
      point: { space: 'page-local', x: 1, y: 1 },
      status: 'unavailable',
      reason: 'page-index-required',
    })
  })

  it('preserves empty, single-line, multi-line, and cross-page selection states', () => {
    const { geometry } = makeGeometry()
    expect(geometry.selectionToGeometry(4, 4)).toEqual({
      from: 4,
      to: 4,
      status: 'empty',
      pages: [],
      sections: [],
      rects: [],
    })
    const selection = geometry.selectionToGeometry(4, 15)
    expect(selection).toMatchObject({
      from: 4,
      to: 15,
      status: 'resolved',
      pages: [0, 1],
      sections: [0, 1],
    })
    expect(selection.rects).toHaveLength(2)
    expect(selection.rects?.[0]).toMatchObject({
      flowRect: { space: 'flow', top: 20 },
      documentRect: { space: 'document', top: 40 },
      pageLocalRect: { space: 'page-local', top: 20 },
    })
    expect(geometry.selectionToGeometry(15, 4).status).toBe('unavailable')
  })

  it('records deterministic reverse mapping and ambiguous/unavailable cases', () => {
    const { geometry } = makeGeometry()
    const snapshot = snapshotPresentationGeometry(geometry, {
      positions: [4, 15],
      selections: [{ from: 4, to: 15 }],
    })
    expect(snapshot.pages).toHaveLength(2)
    expect(snapshot.positions.every((item) => item.status === 'resolved')).toBe(true)
    expect(snapshot.hitTests.every((item) => item.status === 'resolved')).toBe(true)
    expect(snapshot.hitTests.map((item) => item.pmPosition)).toEqual([4, 15])
    expect(compareDiagnosticParity({ geometry: snapshot }, { geometry: snapshot })).toEqual([])
  })

  it('classifies geometry parity failures with coordinate-space context', () => {
    const differences = compareDiagnosticParity(
      {
        geometry: {
          positions: [{ position: 4, documentRect: { space: 'document', top: 10 } }],
          hitTests: [{ point: { space: 'document', x: 1, y: 2 }, pmPosition: 4 }],
          selections: [{ from: 4, to: 15, rects: [] }],
          pages: [{ pageIndex: 0, pageWidth: 100 }],
        },
      },
      {
        geometry: {
          positions: [{ position: 4, documentRect: { space: 'document', top: 12 } }],
          hitTests: [{ point: { space: 'document', x: 1, y: 2 }, pmPosition: 5 }],
          selections: [{ from: 4, to: 15, rects: [{ pageIndex: 0 }] }],
          pages: [{ pageIndex: 0, pageWidth: 101 }],
        },
      },
    )
    expect(differences.map((difference) => difference.category)).toEqual([
      'geometry-hit-test',
      'geometry-page',
      'geometry-position',
      'geometry-selection',
    ])
    expect(
      differences.find((difference) => difference.category === 'geometry-position'),
    ).toMatchObject({
      coordinateSpace: 'document',
      pmPosition: 4,
      delta: 2,
    })
  })
})
