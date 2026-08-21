import { describe, expect, it } from 'vitest'
import { Editor } from '@tiptap/core'
import type { EditorView } from '@tiptap/pm/view'
import { editorExtensions } from '../src/renderer/editor/extensions'
import {
  revisionChangeBarSegments,
  revisionGeometryRangesOf,
  selectionAnchorRectFromGeometry,
  syncMarginAnnotations,
} from '../src/renderer/editor/margin-annotations'
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

function viewportBox(top: number, left: number, width: number, height: number) {
  return {
    top,
    left,
    width,
    height,
    right: left + width,
    bottom: top + height,
  } as DOMRect
}

describe('margin annotation geometry readback', () => {
  it('shows one deleted paragraph balloon when consecutive hidden paragraphs start document', () => {
    const pm = document.createElement('div')
    const wrap = document.createElement('div')
    wrap.className = 'page-wrap rev-balloon'
    wrap.appendChild(pm)
    document.body.appendChild(wrap)
    const editor = new Editor({
      element: pm,
      extensions: editorExtensions,
      content: {
        type: 'doc',
        content: [
          {
            type: 'docParagraph',
            attrs: { docxIndex: null, paraMarkDel: '{"author":"Bob"}' },
            content: [
              {
                type: 'text',
                text: 'deleted',
                marks: [{ type: 'del', attrs: { author: 'Bob' } }],
              },
            ],
          },
          {
            type: 'docParagraph',
            attrs: { docxIndex: null, paraMarkDel: '{"author":"Bob"}' },
            content: [
              {
                type: 'text',
                text: 'deleted again',
                marks: [{ type: 'del', attrs: { author: 'Bob' } }],
              },
            ],
          },
          {
            type: 'docParagraph',
            attrs: { docxIndex: null },
            content: [{ type: 'text', text: 'visible next paragraph' }],
          },
        ],
      },
    })
    const nextParagraph = pm.querySelectorAll('p')[2] as HTMLElement
    Object.defineProperty(wrap, 'getBoundingClientRect', {
      value: () => viewportBox(0, 0, 600, 1000),
    })
    Object.defineProperty(pm, 'getBoundingClientRect', {
      value: () => viewportBox(0, 0, 400, 1000),
    })
    Object.defineProperty(nextParagraph, 'getClientRects', {
      value: () => [viewportBox(40, 80, 200, 16)],
    })

    syncMarginAnnotations(
      wrap,
      pm,
      [],
      1,
      undefined,
      editor.view,
      geometryWith({ status: 'unavailable', rects: [] }, { status: 'unavailable' }),
    )

    const bubble = wrap.querySelector('.rev-bubble') as HTMLElement | null
    expect(bubble).not.toBeNull()
    expect(bubble?.style.top).toBe('40px')
    editor.destroy()
    wrap.remove()
  })

  it('maps visible revision ranges through multi-rect geometry without bridging a page gap', () => {
    const result = revisionChangeBarSegments(
      [{ from: 10, to: 30 }],
      geometryWith(
        {
          status: 'resolved',
          rects: [
            { documentRect: documentRect(20, 8, 30, 14) },
            { documentRect: documentRect(44, 8, 30, 14) },
            { documentRect: documentRect(140, 8, 30, 14) },
          ],
        },
        { status: 'unavailable' },
      ),
      2,
    )

    expect(result).toEqual([
      { top: 10, bottom: 17 },
      { top: 22, bottom: 29 },
      { top: 70, bottom: 77 },
    ])
  })

  it('uses line geometry for a collapsed visible revision decoration', () => {
    const result = revisionChangeBarSegments(
      [{ from: 7, to: 7 }],
      geometryWith(
        { status: 'empty', rects: [] },
        { status: 'resolved', documentRect: documentRect(30, 12, 1, 16) },
      ),
      1,
    )

    expect(result).toEqual([{ top: 30, bottom: 46 }])
  })

  it('skips a non-collapsed revision range when geometry is unavailable', () => {
    const result = revisionChangeBarSegments(
      [{ from: 15, to: 24 }],
      geometryWith({ status: 'unavailable', rects: [] }, { status: 'unavailable' }),
      1,
    )

    expect(result).toEqual([])
  })

  it('collects visible inline and paragraph revision decorations from PM ranges', () => {
    const view = {
      state: {
        doc: {
          descendants(callback: (node: unknown, pos: number) => boolean) {
            callback(
              {
                isText: true,
                nodeSize: 5,
                marks: [{ type: { name: 'ins' } }],
                attrs: {},
              },
              3,
            )
            callback(
              {
                isText: false,
                nodeSize: 12,
                marks: [],
                attrs: { pPrChange: 'format' },
              },
              20,
            )
            callback(
              {
                isText: false,
                nodeSize: 10,
                marks: [],
                attrs: { moveRevision: 'to' },
              },
              40,
            )
            return true
          },
        },
      },
    } as unknown as EditorView

    expect(revisionGeometryRangesOf(view)).toEqual([
      { from: 3, to: 8 },
      { from: 21, to: 31 },
      { from: 41, to: 49 },
    ])
  })

  it('uses the DOM bar fallback only when PresentationGeometry is unavailable', () => {
    const wrap = document.createElement('div')
    const pm = document.createElement('div')
    const revision = document.createElement('span')
    revision.className = 'doc-ins'
    pm.appendChild(revision)
    wrap.appendChild(pm)
    document.body.appendChild(wrap)

    Object.defineProperty(wrap, 'getBoundingClientRect', {
      value: () => ({ top: 100, left: 50, right: 450, bottom: 500, height: 400 }),
    })
    Object.defineProperty(pm, 'getBoundingClientRect', {
      value: () => ({ top: 100, left: 70, right: 430, bottom: 500, height: 400 }),
    })
    Object.defineProperty(revision, 'getClientRects', {
      value: () => [{ top: 110, bottom: 126, height: 16 }],
    })

    syncMarginAnnotations(wrap, pm, [], 2)

    const bar = wrap.querySelector('.change-bar') as HTMLElement | null
    expect(bar?.style.top).toBe('5px')
    expect(bar?.style.height).toBe('8px')
    wrap.remove()
  })

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
