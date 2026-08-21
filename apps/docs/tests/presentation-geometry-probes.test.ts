import { describe, expect, it } from 'vitest'
import {
  capturePresentationGeometryProbes,
  geometryProbeDiagnostics,
  type GeometryProbeDocument,
} from '../src/renderer/presentation-v2/geometry-probes'
import {
  pageIndexFromPageNumber,
  pageNumberFromPageIndex,
} from '../src/renderer/presentation-v2/diagnostics'
import type { PresentationGeometry } from '../src/renderer/presentation-v2/geometry'

const doc: GeometryProbeDocument = {
  type: 'doc',
  content: [
    {
      type: 'docParagraph',
      attrs: { docxIndex: 4 },
      content: [{ type: 'text', text: 'alpha beta' }],
    },
    {
      type: 'docTable',
      attrs: { docxIndex: 9 },
      content: [
        {
          type: 'docTableRow',
          content: [
            {
              type: 'docTableCell',
              content: [{ type: 'docParagraph', content: [{ type: 'text', text: 'cell text' }] }],
            },
          ],
        },
      ],
    },
    {
      type: 'docParagraph',
      content: [
        {
          type: 'text',
          text: 'commented',
          marks: [{ type: 'comment', attrs: { ids: '7' } }],
        },
      ],
    },
  ],
}

function fakeGeometry(): PresentationGeometry {
  const positionToGeometry = (position: number) => {
    const pageIndex = position >= 10 ? 1 : 0
    return {
      position,
      status: 'resolved' as const,
      pageIndex,
      sectionIndex: pageIndex,
      columnIndex: position >= 10 ? 1 : 0,
      columnCount: 2,
      documentRect: {
        space: 'document' as const,
        left: position,
        top: position * 2,
        width: 1,
        height: 12,
        right: position + 1,
        bottom: position * 2 + 12,
      },
      caretRect: {
        space: 'document' as const,
        left: position,
        top: position * 2,
        width: 1,
        height: 12,
        right: position + 1,
        bottom: position * 2 + 12,
      },
      line: { index: position % 2, flowTop: position * 2, height: 12 },
      block: {
        index: 0,
        flowRect: {
          space: 'flow' as const,
          left: 0,
          top: 0,
          width: 100,
          height: 20,
          right: 100,
          bottom: 20,
        },
      },
    }
  }
  return {
    coordinateSpaces: {
      viewport: 'browser-viewport-css-px',
      document: 'page-wrap-relative-css-px',
      pageLocal: 'page-relative-css-px',
      flow: 'gapless-layout-px-at-100-percent',
      zoomFactor: 1,
    },
    pageCount: 2,
    positionToGeometry,
    locatePosition: positionToGeometry,
    pointToPosition: (point) => ({
      point,
      status: 'resolved' as const,
      pmPosition: Math.floor(point.x),
      pageIndex: point.x >= 10 ? 1 : 0,
    }),
    selectionToGeometry: (from, to) => ({
      from,
      to,
      status: 'resolved' as const,
      pages: [from >= 10 ? 1 : 0, to >= 10 ? 1 : 0],
      sections: [from >= 10 ? 1 : 0],
      rects: [],
    }),
    pageGeometry: () => undefined,
  }
}

describe('deterministic Presentation Geometry probes', () => {
  it('resolves structural anchors and validates geometry round trips', () => {
    const geometry = fakeGeometry()
    const results = capturePresentationGeometryProbes(geometry, doc, [
      {
        id: 'paragraph-middle',
        fixtureId: 'unit',
        semanticCase: 'paragraph-middle',
        anchor: { kind: 'node', nodeType: 'docParagraph', occurrence: 0, offset: 'middle' },
        expected: { nodeType: 'docParagraph', docxIndex: 4 },
      },
      {
        id: 'table-cell',
        fixtureId: 'unit',
        semanticCase: 'table-cell',
        anchor: { kind: 'table-cell', tableDocxIndex: 9, row: 0, cell: 0, offset: 'middle' },
        expected: { nodeType: 'docTable', table: { row: 0, cell: 0 } },
      },
      {
        id: 'comment-range',
        fixtureId: 'unit',
        semanticCase: 'comment-range',
        anchor: { kind: 'mark-range', markType: 'comment', occurrence: 0 },
        expected: { markType: 'comment' },
      },
      {
        id: 'page-before-gap',
        fixtureId: 'unit',
        semanticCase: 'page-before-gap',
        anchor: { kind: 'page-boundary', pageIndex: 1, side: 'before-gap' },
        expected: { pageIndex: 0 },
      },
      {
        id: 'page-after-gap',
        fixtureId: 'unit',
        semanticCase: 'page-after-gap',
        anchor: { kind: 'page-boundary', pageIndex: 1, side: 'after-gap' },
        expected: { pageIndex: 1 },
      },
      {
        id: 'header',
        fixtureId: 'unit',
        semanticCase: 'header',
        anchor: { kind: 'header-footer', part: 'header' },
        optional: true,
      },
    ])

    expect(results.map((result) => result.status)).toEqual([
      'resolved',
      'resolved',
      'resolved',
      'resolved',
      'resolved',
      'unavailable',
    ])
    expect(results[0]).toMatchObject({
      pmPosition: 6,
      pageIndex: 0,
      pageNumber: 1,
      roundTrip: { status: 'exact' },
    })
    expect(results[1].structuralContext?.table).toEqual({ docxIndex: 9, row: 0, cell: 0 })
    expect(results[2].pmRange?.to).toBeGreaterThan(results[2].pmRange?.from ?? 0)
    expect(results[3].pmPosition).toBeLessThan(results[4].pmPosition ?? 0)
    expect(results[5].reason).toBe('presentation-only-no-pm-position')
    expect(geometryProbeDiagnostics(results)).toEqual([])
  })

  it('keeps canonical page indexes distinct from legacy diagnostic numbers', () => {
    expect(pageNumberFromPageIndex(0)).toBe(1)
    expect(pageNumberFromPageIndex(4)).toBe(5)
    expect(pageIndexFromPageNumber(1)).toBe(0)
    expect(pageIndexFromPageNumber(5)).toBe(4)
    expect(pageIndexFromPageNumber(0)).toBeUndefined()
  })
})
