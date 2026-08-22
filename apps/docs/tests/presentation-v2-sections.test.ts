import { describe, expect, it } from 'vitest'
import { renderPresentationV1, renderPresentationV2 } from '../src/renderer/presentation-v2'
import type { BlockBox, SectionGeom } from '../src/renderer/pagination'
import { normalizePresentationSectionsV2 } from '../src/renderer/presentation-v2/sections'

function block(top: number, height: number, section: number, docxIndex: number): BlockBox {
  return {
    top,
    height,
    section,
    docxIndex,
    lineBoxes: [{ offsetInBlock: 0, height }],
  }
}

function renderPair(sectionGeoms: SectionGeom[], blocks: BlockBox[], totalHeight: number) {
  const input = () => ({
    blocks: blocks.map((item) => ({
      ...item,
      lineBoxes: item.lineBoxes?.map((line) => ({ ...line })),
    })),
    sectionGeoms,
    totalHeight,
    zoomFactor: 1,
  })
  return [renderPresentationV1(input()), renderPresentationV2(input())] as const
}

describe('Presentation V2 section normalization', () => {
  it('canonicalizes derived section flow inputs without mutating SectionGeom values', () => {
    const sectionGeoms: SectionGeom[] = [
      { contentHeight: 700, forceBreak: false },
      { contentHeight: 680, forceBreak: true, startType: 'nextPage' },
      {
        contentHeight: 660,
        forceBreak: false,
        startType: 'continuous',
        cols: 2,
        colWidths: [240, 180],
        colBreakStart: true,
      },
    ]
    const original = sectionGeoms.map((geom) => ({
      ...geom,
      colWidths: geom.colWidths ? [...geom.colWidths] : undefined,
    }))

    const normalized = normalizePresentationSectionsV2(sectionGeoms)

    expect(normalized.geoms).not.toBe(sectionGeoms)
    expect(normalized.geoms[0]).toMatchObject({
      contentHeight: 700,
      forceBreak: false,
      cols: 1,
      colBreakStart: false,
    })
    expect(normalized.geoms[1]).toMatchObject({ contentHeight: 680, startType: 'nextPage' })
    expect(normalized.geoms[2]).toMatchObject({
      contentHeight: 660,
      cols: 2,
      colWidths: [240, 180],
      colBreakStart: true,
    })
    expect(normalized.transitions).toEqual(['initial', 'page', 'column'])
    expect(normalized.hasUsableGeometry).toBe(true)
    expect(sectionGeoms).toEqual(original)
    expect(normalized.geoms[2].colWidths).not.toBe(sectionGeoms[2].colWidths)
  })

  it('preserves the zero-capacity fallback condition', () => {
    const normalized = normalizePresentationSectionsV2([
      { contentHeight: 0, forceBreak: false },
      { contentHeight: -1, forceBreak: true },
    ])

    expect(normalized.hasUsableGeometry).toBe(false)
    expect(normalized.geoms.map((geom) => geom.contentHeight)).toEqual([0, -1])
  })

  it.each<{
    name: string
    geoms: SectionGeom[]
    blocks: BlockBox[]
    totalHeight: number
  }>([
    {
      name: 'single section',
      geoms: [{ contentHeight: 100, forceBreak: false }],
      blocks: [block(0, 80, 0, 0), block(80, 80, 0, 1)],
      totalHeight: 160,
    },
    {
      name: 'next page section',
      geoms: [
        { contentHeight: 100, forceBreak: false },
        { contentHeight: 100, forceBreak: true, startType: 'nextPage' },
      ],
      blocks: [block(0, 80, 0, 0), block(80, 80, 1, 1)],
      totalHeight: 160,
    },
    {
      name: 'continuous section',
      geoms: [
        { contentHeight: 100, forceBreak: false },
        { contentHeight: 100, forceBreak: false, startType: 'continuous' },
      ],
      blocks: [block(0, 80, 0, 0), block(80, 80, 1, 1)],
      totalHeight: 160,
    },
    {
      name: 'continuous changed-page geometry',
      geoms: [
        { contentHeight: 100, forceBreak: false },
        { contentHeight: 120, forceBreak: true, startType: 'continuous' },
      ],
      blocks: [block(0, 80, 0, 0), block(80, 80, 1, 1)],
      totalHeight: 160,
    },
    {
      name: 'even-page section',
      geoms: [
        { contentHeight: 80, forceBreak: false },
        { contentHeight: 80, forceBreak: true, startType: 'evenPage' },
      ],
      blocks: [block(0, 160, 0, 0), block(160, 40, 1, 1)],
      totalHeight: 200,
    },
    {
      name: 'odd-page section',
      geoms: [
        { contentHeight: 100, forceBreak: false },
        { contentHeight: 100, forceBreak: true, startType: 'oddPage' },
      ],
      blocks: [block(0, 80, 0, 0), block(80, 40, 1, 1)],
      totalHeight: 120,
    },
    {
      name: 'compatible next-column section',
      geoms: [
        { contentHeight: 100, forceBreak: false, cols: 2 },
        { contentHeight: 100, forceBreak: false, cols: 2, colBreakStart: true },
      ],
      blocks: [block(0, 80, 0, 0), block(80, 80, 1, 1)],
      totalHeight: 160,
    },
    {
      name: 'incompatible next-column section',
      geoms: [
        { contentHeight: 100, forceBreak: false, cols: 2 },
        { contentHeight: 100, forceBreak: true, startType: 'nextColumn', cols: 1 },
      ],
      blocks: [block(0, 80, 0, 0), block(80, 80, 1, 1)],
      totalHeight: 160,
    },
    {
      name: 'single to multi-column section',
      geoms: [
        { contentHeight: 100, forceBreak: false },
        { contentHeight: 100, forceBreak: false, cols: 2 },
      ],
      blocks: [block(0, 80, 0, 0), block(80, 80, 1, 1)],
      totalHeight: 160,
    },
    {
      name: 'multi to single-column section',
      geoms: [
        { contentHeight: 100, forceBreak: false, cols: 2 },
        { contentHeight: 100, forceBreak: false },
      ],
      blocks: [block(0, 80, 0, 0), block(80, 80, 1, 1)],
      totalHeight: 160,
    },
    {
      name: 'header-footer-reduced capacity',
      geoms: [
        { contentHeight: 100, forceBreak: false },
        { contentHeight: 60, forceBreak: true, startType: 'nextPage' },
      ],
      blocks: [block(0, 80, 0, 0), block(80, 80, 1, 1)],
      totalHeight: 160,
    },
    {
      name: 'explicit-width columns',
      geoms: [{ contentHeight: 100, forceBreak: false, cols: 2, colWidths: [240, 180] }],
      blocks: [block(0, 80, 0, 0), block(80, 80, 0, 1)],
      totalHeight: 160,
    },
  ])('keeps V1 and V2 PageSlice output identical for $name', ({ geoms, blocks, totalHeight }) => {
    const [v1, v2] = renderPair(geoms, blocks, totalHeight)
    expect(v2).toEqual(v1)
  })
})
