import type { SectionInfo } from '@genoffice/docx-engine'
import { sectionPageBox, type BlockBox, type PageSlice } from '../pagination'
import type { PositionDiagnosticsView } from './diagnostics'

/**
 * Geometry coordinates are explicit:
 * - `viewport`: browser viewport CSS pixels; accepted only as hit-test input.
 * - `document`: page-wrap-relative rendered CSS pixels (physical document space).
 * - `page-local`: rendered CSS pixels from a page's physical top-left origin.
 * - `flow`: gapless layout pixels at 100% zoom, matching pagination coordinates.
 *
 * Viewport coordinates are converted through the page-wrap/flow origins and are
 * never returned in normalized geometry. Scroll offsets therefore cannot leak
 * into diagnostics.
 */
export type GeometryCoordinateSpace = 'viewport' | 'document' | 'page-local' | 'flow'

export interface GeometryCoordinateSpaces {
  viewport: 'browser-viewport-css-px'
  document: 'page-wrap-relative-css-px'
  pageLocal: 'page-relative-css-px'
  flow: 'gapless-layout-px-at-100-percent'
  zoomFactor: number
}

export interface GeometryPoint {
  space: GeometryCoordinateSpace
  x: number
  y: number
  /** Required for `page-local`; page indexes are zero-based like `PageSlice[]`. */
  pageIndex?: number
}

export interface GeometryRect {
  space: GeometryCoordinateSpace
  left: number
  top: number
  width: number
  height: number
  right: number
  bottom: number
}

export type GeometryMappingStatus = 'resolved' | 'empty' | 'unavailable'

export interface GeometryLineAssociation {
  index: number
  flowTop: number
  height: number
}

export interface GeometryBlockAssociation {
  index: number
  docxIndex?: number
  flowRect: GeometryRect
}

export interface PositionGeometry {
  position: number
  status: GeometryMappingStatus
  /** Zero-based `PageSlice[]` index. */
  pageIndex?: number
  sectionIndex?: number
  flowRect?: GeometryRect
  documentRect?: GeometryRect
  pageLocalRect?: GeometryRect
  caretRect?: GeometryRect
  line?: GeometryLineAssociation
  block?: GeometryBlockAssociation
  reason?: 'editor-coordinates-unavailable'
}

export interface PointToPositionResult {
  point: GeometryPoint
  status: 'resolved' | 'unavailable'
  pageIndex?: number
  sectionIndex?: number
  pmPosition?: number
  /** ProseMirror's `inside` result, when its hit-test API supplies one. */
  inside?: number
  /** Preserved only when an underlying editor implementation supplies it. */
  affinity?: string
  bias?: number
  reason?: 'editor-hit-test-unavailable' | 'page-index-required' | 'unsupported-space'
}

export interface GeometrySelectionRect {
  pageIndex?: number
  sectionIndex?: number
  flowRect: GeometryRect
  documentRect: GeometryRect
  pageLocalRect?: GeometryRect
}

export interface SelectionGeometry {
  from: number
  to: number
  status: GeometryMappingStatus
  pages: number[]
  sections: number[]
  /** Omitted when a non-empty range has no deterministic browser geometry. */
  rects?: GeometrySelectionRect[]
  reason?: 'editor-selection-unavailable'
}

export interface PageGapGeometry {
  pageIndex: number
  boundary: { fromPageIndex: number; toPageIndex: number }
  documentRect: GeometryRect
  bandRect?: GeometryRect
  margins: { top: number; bottom: number }
}

export interface PageGeometry {
  /** Zero-based `PageSlice[]` index; existing diagnostics may remain one-based. */
  pageIndex: number
  sectionIndex: number
  pageWidth?: number
  pageHeight?: number
  physicalOrigin?: GeometryPoint
  documentRect?: GeometryRect
  pageLocalOrigin: GeometryPoint
  flowRect: GeometryRect
  flowOffset: GeometryPoint
  gapBefore?: PageGapGeometry
}

export interface PresentationGeometrySource {
  root: HTMLElement
  flowRoot: HTMLElement
  slices: readonly PageSlice[]
  editorView?: PositionDiagnosticsView & {
    dom?: HTMLElement
    domAtPos?: (position: number, side?: number) => { node: Node; offset: number }
  }
  blocks?: readonly BlockBox[]
  sections?: readonly SectionInfo[]
  zoomFactor?: number
}

export interface PresentationGeometry {
  readonly coordinateSpaces: GeometryCoordinateSpaces
  readonly pageCount: number
  positionToGeometry(pmPos: number, side?: number): PositionGeometry
  locatePosition(pmPos: number, side?: number): PositionGeometry
  pointToPosition(point: GeometryPoint): PointToPositionResult
  selectionToGeometry(from: number, to: number): SelectionGeometry
  pageGeometry(pageIndex: number): PageGeometry | undefined
}

export interface GeometrySnapshot {
  coordinateSpaces: GeometryCoordinateSpaces
  pages: PageGeometry[]
  positions: PositionGeometry[]
  hitTests: PointToPositionResult[]
  selections: SelectionGeometry[]
}

export interface GeometrySnapshotProbes {
  positions?: readonly number[]
  selections?: ReadonlyArray<{ from: number; to: number }>
  /** Explicit points are useful for reverse hit-test diagnostics. */
  points?: readonly GeometryPoint[]
}

function finite(value: number): number | undefined {
  return Number.isFinite(value) ? value : undefined
}

function cssPx(el: HTMLElement, name: string): number | undefined {
  const direct = parseFloat(el.style.getPropertyValue(name))
  if (Number.isFinite(direct)) return direct
  if (typeof getComputedStyle !== 'function') return undefined
  return finite(parseFloat(getComputedStyle(el).getPropertyValue(name)))
}

function pageRectFromViewport(rect: DOMRect, rootRect: DOMRect): GeometryRect {
  return rectFromValues(
    'document',
    rect.left - rootRect.left,
    rect.top - rootRect.top,
    rect.width,
    rect.height,
  )
}

function rectFromValues(
  space: GeometryCoordinateSpace,
  left: number,
  top: number,
  width: number,
  height: number,
): GeometryRect {
  return { space, left, top, width, height, right: left + width, bottom: top + height }
}

function flowRectFromViewport(
  rect: DOMRect,
  flowRootRect: DOMRect,
  zoom: number,
): GeometryRect {
  return rectFromValues(
    'flow',
    (rect.left - flowRootRect.left) / zoom,
    (rect.top - flowRootRect.top) / zoom,
    rect.width / zoom,
    rect.height / zoom,
  )
}

function pageLocalRect(documentRect: GeometryRect, pageRect: GeometryRect): GeometryRect {
  return rectFromValues(
    'page-local',
    documentRect.left - pageRect.left,
    documentRect.top - pageRect.top,
    documentRect.width,
    documentRect.height,
  )
}

function pointInRect(y: number, rect: GeometryRect): boolean {
  return y >= rect.top && y <= rect.bottom
}

function rangeRects(
  view: PresentationGeometrySource['editorView'],
  from: number,
  to: number,
): DOMRect[] {
  if (!view || typeof document === 'undefined') return []
  if (view.domAtPos) {
    try {
      const start = view.domAtPos(from, 1)
      const end = view.domAtPos(to, -1)
      const range = document.createRange()
      range.setStart(start.node, start.offset)
      range.setEnd(end.node, end.offset)
      return [...range.getClientRects()].filter((rect) => rect.width > 0 && rect.height > 0)
    } catch {
      return []
    }
  }
  const native = globalThis.getSelection?.()
  if (native && native.rangeCount > 0) {
    const range = native.getRangeAt(0)
    if (view.dom?.contains(range.commonAncestorContainer))
      return [...range.getClientRects()].filter((rect) => rect.width > 0 && rect.height > 0)
  }
  return []
}

function pageHeightOf(page: HTMLElement, zoom: number): number | undefined {
  const value = cssPx(page, '--page-h') ?? cssPx(page, 'min-height')
  return value === undefined ? finite(page.getBoundingClientRect().height) : value * zoom
}

function normalizePageIndex(pageIndex: number, count: number): number | undefined {
  return Number.isInteger(pageIndex) && pageIndex >= 0 && pageIndex < count ? pageIndex : undefined
}

export function createPresentationGeometry(source: PresentationGeometrySource): PresentationGeometry {
  const zoom = source.zoomFactor ?? 1
  const rootRect = source.root.getBoundingClientRect()
  const flowRootRect = source.flowRoot.getBoundingClientRect()
  const firstPage = source.root.querySelector<HTMLElement>(':scope > .doc-page') ?? source.flowRoot
  const firstPageRect = firstPage.getBoundingClientRect()
  const pageWidth = finite(firstPageRect.width)
  const pageHeight = pageHeightOf(firstPage, zoom)
  const gapElements = Array.from(
    source.root.querySelectorAll<HTMLElement>('.page-gap, .page-gap-cut'),
  )
  const gapByPage = new Map<number, PageGapGeometry>()

  for (const [index, element] of gapElements.entries()) {
    const pageIndex = Math.min(index + 1, Math.max(0, source.slices.length - 1))
    const documentRect = pageRectFromViewport(element.getBoundingClientRect(), rootRect)
    const top = cssPx(element, '--gap-mt') ?? 0
    const bottom = cssPx(element, '--gap-mb') ?? 0
    const bandTop = documentRect.top + bottom * zoom
    const bandBottom = documentRect.bottom - top * zoom
    gapByPage.set(pageIndex, {
      pageIndex,
      boundary: { fromPageIndex: Math.max(0, pageIndex - 1), toPageIndex: pageIndex },
      documentRect,
      ...(bandBottom > bandTop
        ? { bandRect: rectFromValues('document', documentRect.left, bandTop, documentRect.width, bandBottom - bandTop) }
        : {}),
      margins: { top, bottom },
    })
  }

  const pages = source.slices.map((slice, pageIndex): PageGeometry => {
    const gap = gapByPage.get(pageIndex)
    const sectionBox = source.sections?.[slice.section]?.settings
      ? sectionPageBox(source.sections[slice.section].settings)
      : undefined
    const renderedPageWidth = sectionBox?.width !== undefined ? sectionBox.width * zoom : pageWidth
    const renderedPageHeight = sectionBox?.height !== undefined ? sectionBox.height * zoom : pageHeight
    const pageTop =
      pageIndex === 0
        ? firstPageRect.top - rootRect.top
        : gap
          ? gap.documentRect.bottom - gap.margins.top * zoom
          : undefined
    const documentRect =
      pageTop === undefined || renderedPageWidth === undefined || renderedPageHeight === undefined
        ? undefined
        : rectFromValues(
            'document',
            firstPageRect.left - rootRect.left,
            pageTop,
            renderedPageWidth,
            renderedPageHeight,
          )
    const flowRect = rectFromValues(
      'flow',
      0,
      slice.start,
      flowRootRect.width / zoom,
      slice.end - slice.start,
    )
    return {
      pageIndex,
      sectionIndex: slice.section,
      ...(renderedPageWidth !== undefined ? { pageWidth: renderedPageWidth } : {}),
      ...(renderedPageHeight !== undefined ? { pageHeight: renderedPageHeight } : {}),
      ...(documentRect
        ? {
            physicalOrigin: { space: 'document' as const, x: documentRect.left, y: documentRect.top },
            documentRect,
          }
        : {}),
      pageLocalOrigin: { space: 'page-local', x: 0, y: 0, pageIndex },
      flowRect,
      flowOffset: { space: 'flow', x: 0, y: slice.start },
      ...(gap ? { gapBefore: gap } : {}),
    }
  })

  const pageForFlowY = (y: number): number | undefined => {
    if (source.slices.length === 0) return undefined
    let pageIndex = 0
    for (let index = 1; index < source.slices.length; index += 1) {
      if (y >= source.slices[index].start) pageIndex = index
    }
    return pageIndex
  }

  const pageForDocumentY = (y: number): number | undefined =>
    pages.find((page) => page.documentRect && pointInRect(y, page.documentRect))?.pageIndex

  const sectionForPage = (pageIndex: number | undefined): number | undefined =>
    pageIndex === undefined ? undefined : pages[pageIndex]?.sectionIndex

  const blockForFlowY = (y: number): { index: number; block: BlockBox } | undefined => {
    if (!source.blocks) return undefined
    for (const [index, block] of source.blocks.entries()) {
      if (y >= block.top - 0.01 && y <= block.top + block.height + 0.01) return { index, block }
    }
    return undefined
  }

  const lineForFlowY = (
    y: number,
    blockMatch: { index: number; block: BlockBox } | undefined,
  ): GeometryLineAssociation | undefined => {
    const lines = blockMatch?.block.lineBoxes
    if (!lines) return undefined
    for (const [index, line] of lines.entries()) {
      const top = blockMatch.block.top + line.offsetInBlock
      if (y >= top - 0.01 && y <= top + line.height + 0.01)
        return { index, flowTop: top, height: line.height }
    }
    return undefined
  }

  const pageFromGeometry = (flowRect: GeometryRect, documentRect: GeometryRect): number | undefined =>
    pageForFlowY(flowRect.top) ?? pageForDocumentY(documentRect.top)

  const positionToGeometry = (pmPos: number, side = 1): PositionGeometry => {
    if (!source.editorView) return { position: pmPos, status: 'unavailable', reason: 'editor-coordinates-unavailable' }
    try {
      const viewportRect = source.editorView.coordsAtPos(pmPos, side)
      const documentRect = pageRectFromViewport(viewportRect as DOMRect, rootRect)
      const flowRect = flowRectFromViewport(viewportRect as DOMRect, flowRootRect, zoom)
      const pageIndex = pageFromGeometry(flowRect, documentRect)
      const page = pageIndex === undefined ? undefined : pages[pageIndex]
      const block = blockForFlowY(flowRect.top)
      const line = lineForFlowY(flowRect.top, block)
      return {
        position: pmPos,
        status: 'resolved',
        ...(pageIndex !== undefined ? { pageIndex, sectionIndex: sectionForPage(pageIndex) } : {}),
        flowRect,
        documentRect,
        ...(page?.documentRect ? { pageLocalRect: pageLocalRect(documentRect, page.documentRect) } : {}),
        caretRect: documentRect,
        ...(line ? { line } : {}),
        ...(block
          ? {
              block: {
                index: block.index,
                ...(block.block.docxIndex !== undefined ? { docxIndex: block.block.docxIndex } : {}),
                flowRect: rectFromValues(
                  'flow',
                  0,
                  block.block.top,
                  flowRootRect.width / zoom,
                  block.block.height,
                ),
              },
            }
          : {}),
      }
    } catch {
      return { position: pmPos, status: 'unavailable', reason: 'editor-coordinates-unavailable' }
    }
  }

  const documentPointToViewport = (point: GeometryPoint): { x: number; y: number } | undefined => {
    if (point.space === 'viewport') return { x: point.x, y: point.y }
    if (point.space === 'document') return { x: rootRect.left + point.x, y: rootRect.top + point.y }
    if (point.space === 'flow') {
      return { x: flowRootRect.left + point.x * zoom, y: flowRootRect.top + point.y * zoom }
    }
    const pageIndex = normalizePageIndex(point.pageIndex ?? -1, pages.length)
    const page = pageIndex === undefined ? undefined : pages[pageIndex]
    if (!page?.documentRect) return undefined
    return { x: rootRect.left + page.documentRect.left + point.x, y: rootRect.top + page.documentRect.top + point.y }
  }

  const pointToPosition = (point: GeometryPoint): PointToPositionResult => {
    if (point.space === 'page-local' && point.pageIndex === undefined)
      return { point, status: 'unavailable', reason: 'page-index-required' }
    const viewport = documentPointToViewport(point)
    if (!viewport || !source.editorView) {
      return {
        point,
        status: 'unavailable',
        reason: viewport ? 'editor-hit-test-unavailable' : 'unsupported-space',
      }
    }
    const pageIndex =
      point.space === 'flow'
        ? pageForFlowY(point.y)
        : point.space === 'document' || point.space === 'viewport'
          ? pageForDocumentY(point.space === 'viewport' ? point.y - rootRect.top : point.y)
          : normalizePageIndex(point.pageIndex ?? -1, pages.length)
    try {
      const hit = source.editorView.posAtCoords({ left: viewport.x, top: viewport.y }) as
        | ({ pos: number; inside?: number; affinity?: string; bias?: number } | null)
        | null
      if (!hit || typeof hit.pos !== 'number')
        return { point, status: 'unavailable', reason: 'editor-hit-test-unavailable' }
      return {
        point,
        status: 'resolved',
        ...(pageIndex !== undefined ? { pageIndex, sectionIndex: sectionForPage(pageIndex) } : {}),
        pmPosition: hit.pos,
        ...(hit.inside !== undefined ? { inside: hit.inside } : {}),
        ...(hit.affinity !== undefined ? { affinity: hit.affinity } : {}),
        ...(hit.bias !== undefined ? { bias: hit.bias } : {}),
      }
    } catch {
      return { point, status: 'unavailable', reason: 'editor-hit-test-unavailable' }
    }
  }

  const selectionToGeometry = (from: number, to: number): SelectionGeometry => {
    if (from === to) return { from, to, status: 'empty', pages: [], sections: [], rects: [] }
    if (from > to) return { from, to, status: 'unavailable', pages: [], sections: [], reason: 'editor-selection-unavailable' }
    const rects = rangeRects(source.editorView, from, to).map((viewportRect) => {
      const documentRect = pageRectFromViewport(viewportRect, rootRect)
      const flowRect = flowRectFromViewport(viewportRect, flowRootRect, zoom)
      const pageIndex = pageFromGeometry(flowRect, documentRect)
      const page = pageIndex === undefined ? undefined : pages[pageIndex]
      return {
        ...(pageIndex !== undefined ? { pageIndex, sectionIndex: sectionForPage(pageIndex) } : {}),
        flowRect,
        documentRect,
        ...(page?.documentRect ? { pageLocalRect: pageLocalRect(documentRect, page.documentRect) } : {}),
      }
    })
    const pagesForRects = [...new Set(rects.map((rect) => rect.pageIndex).filter((value): value is number => value !== undefined))]
    const sectionsForRects = [...new Set(rects.map((rect) => rect.sectionIndex).filter((value): value is number => value !== undefined))]
    return rects.length > 0
      ? { from, to, status: 'resolved', pages: pagesForRects, sections: sectionsForRects, rects }
      : { from, to, status: 'unavailable', pages: [], sections: [], reason: 'editor-selection-unavailable' }
  }

  const api: PresentationGeometry = {
    coordinateSpaces: {
      viewport: 'browser-viewport-css-px',
      document: 'page-wrap-relative-css-px',
      pageLocal: 'page-relative-css-px',
      flow: 'gapless-layout-px-at-100-percent',
      zoomFactor: zoom,
    },
    pageCount: pages.length,
    positionToGeometry,
    locatePosition: positionToGeometry,
    pointToPosition,
    selectionToGeometry,
    pageGeometry: (pageIndex) => {
      const index = normalizePageIndex(pageIndex, pages.length)
      return index === undefined ? undefined : pages[index]
    },
  }
  return api
}

export function snapshotPresentationGeometry(
  geometry: PresentationGeometry,
  probes: GeometrySnapshotProbes = {},
): GeometrySnapshot {
  const positions = (probes.positions ?? []).map((position) => geometry.positionToGeometry(position))
  const hitTests = [
    ...(probes.points ?? []).map((point) => geometry.pointToPosition(point)),
    ...positions
      .filter((position) => position.status === 'resolved' && position.documentRect)
      .map((position) =>
        geometry.pointToPosition({
          space: 'document',
          x: position.documentRect!.left,
          y: position.documentRect!.top,
        }),
      ),
  ]
  return {
    coordinateSpaces: geometry.coordinateSpaces,
    pages: Array.from({ length: geometry.pageCount }, (_, pageIndex) => geometry.pageGeometry(pageIndex)!),
    positions,
    hitTests,
    selections: (probes.selections ?? []).map(({ from, to }) => geometry.selectionToGeometry(from, to)),
  }
}
