import { describe, expect, it } from 'vitest'
import { selectionAnchorRectFromGeometry } from '../src/renderer/editor/margin-annotations'
import type { PresentationGeometry } from '../src/renderer/presentation-v2/geometry'

const rootRect = { top: 100, left: 50 }

function documentRect(top: number, left: number, width: number, height: number) {
  return {
    space: 'document' as const,
    top,
    left,
    width,
    height,
    right: left + width,
    bottom: top + height,
  }
}

function geometryWith(selection: unknown, position: unknown): PresentationGeometry {
  return {
    selectionToGeometry: () => selection,
    positionToGeometry: () => position,
  } as unknown as PresentationGeometry
}

describe('margin annotation geometry readback', () => {
  it('keeps multi-rect selections intact and anchors existing bubbles to first line', () => {
    const result = selectionAnchorRectFromGeometry(
      geometryWith(
        {
          status: 'resolved',
          rects: [
            { documentRect: documentRect(20, 8, 30, 14) },
            { documentRect: documentRect(140, 8, 20, 14) },
          ],
        },
        { status: 'unavailable' },
      ),
      rootRect,
      4,
      15,
    )

    expect(result).toEqual({ top: 120, bottom: 134, left: 58, right: 88, height: 14 })
  })

  it('uses position geometry for collapsed caret anchors', () => {
    const result = selectionAnchorRectFromGeometry(
      geometryWith(
        { status: 'empty', rects: [] },
        { status: 'resolved', documentRect: documentRect(30, 12, 1, 16) },
      ),
      rootRect,
      7,
      7,
    )

    expect(result).toEqual({ top: 130, bottom: 146, left: 62, right: 63, height: 16 })
  })

  it('returns no anchor when selection and caret geometry are unavailable', () => {
    const result = selectionAnchorRectFromGeometry(
      geometryWith({ status: 'unavailable', rects: [] }, { status: 'unavailable' }),
      rootRect,
      4,
      15,
    )

    expect(result).toBeUndefined()
  })
})
