import type { BlockBox, PageSlice } from '../pagination'
import type { SectionInfo } from '@genoffice/docx-engine'
import type { DiagnosticRange, DiagnosticRect, PositionDiagnosticsView } from './diagnostics'
import {
  createPresentationGeometry,
  snapshotPresentationGeometry,
  type GeometrySnapshot,
} from './geometry'
import {
  capturePresentationGeometryProbes,
  type GeometryProbe,
  type GeometryProbeDocument,
  type GeometryProbePointResolver,
  type GeometryProbeResult,
} from './geometry-probes'

/**
 * Coordinates retained by post-render diagnostics. DOM viewport coordinates are
 * read once, then translated by the page-wrap origin; viewport/scroll offsets
 * never enter the normalized snapshot.
 */
export interface PostRenderCoordinateSpaces {
  document: 'page-wrap-relative-css-px'
  flow: 'gapless-layout-px-at-100-percent'
  viewport: 'discarded-after-normalization'
  pageIndex: 'zero-based'
  pageNumber: 'one-based-legacy-diagnostic'
  zoomFactor: number
}

export interface NormalizedPostRenderPage {
  /** Legacy one-based diagnostic page number. */
  page: number
  /** Canonical zero-based PageSlice index. */
  pageIndex: number
  section: number
  /** Logical gapless flow range, in layout px at 100% zoom. */
  flowRect: DiagnosticRect
  /** Actual rendered paper rectangle, relative to the page-wrap origin. */
  pageRect?: DiagnosticRect
  /** Physical offset from flowRect.top/left to pageRect.top/left, in CSS px. */
  flowToPhysicalOffset?: { x: number; y: number }
}

export interface NormalizedPostRenderPageGap {
  /** Legacy one-based diagnostic page number. */
  page: number
  /** Canonical zero-based PageSlice index. */
  pageIndex: number
  boundary: { fromPage: number; toPage: number }
  kind: 'block' | 'inline' | 'table' | 'cell' | 'cut'
  /** Actual decoration rectangle, relative to page-wrap. */
  pageRect: DiagnosticRect
  /** Actual painted inter-page band, when the decoration exposes one. */
  bandRect?: DiagnosticRect
  /** Logical margins encoded by the existing page-gap decoration. */
  margins: { top: number; bottom: number }
  /** Actual rendered decoration height in CSS px. */
  sizePx: number
  flowBoundary?: number
}

export interface NormalizedPostRenderHeaderFooter {
  /** Legacy one-based diagnostic page number. */
  page: number
  /** Canonical zero-based PageSlice index. */
  pageIndex: number
  section: number
  kind: 'header' | 'footer'
  variant?: 'default' | 'first' | 'even'
  /** Actual rendered rectangle, relative to page-wrap. */
  pageRect: DiagnosticRect
  /** Reserved geometry is omitted when the live DOM does not expose it. */
  reservedRect?: DiagnosticRect
}

export interface NormalizedPostRenderFloat {
  /** Legacy one-based diagnostic page number. */
  page?: number
  /** Canonical zero-based page index. */
  pageIndex?: number
  section?: number
  kind: 'body' | 'cell' | 'header-footer'
  block?: number
  anchor?: string
  /** Logical pre-gap/pre-shift geometry, when the pagination pass supplied it. */
  flowRect?: DiagnosticRect
  /** Actual rendered rectangle, relative to page-wrap. */
  pageRect: DiagnosticRect
  /** Existing --page-float-dy/data-page-float-dy correction, in layout px. */
  domShiftY?: number
}

export interface NormalizedPostRenderCaret {
  position: number
  /** Legacy one-based diagnostic page number. */
  page?: number
  /** Canonical zero-based page index. */
  pageIndex?: number
  section?: number
  flowRect?: DiagnosticRect
  pageRect?: DiagnosticRect
  pageLocalRect?: DiagnosticRect
}

export interface NormalizedPostRenderSelection {
  pmRange: DiagnosticRange
  pages: number[]
  pageIndexes: number[]
  sections: number[]
  rects?: Array<{
    page?: number
    pageIndex?: number
    section?: number
    flowRect: DiagnosticRect
    pageRect: DiagnosticRect
    pageLocalRect?: DiagnosticRect
  }>
}

export interface NormalizedPostRenderDiagnostics {
  coordinateSpaces: PostRenderCoordinateSpaces
  pages: NormalizedPostRenderPage[]
  pageGaps: NormalizedPostRenderPageGap[]
  headerFooters: NormalizedPostRenderHeaderFooter[]
  floats: NormalizedPostRenderFloat[]
  caret?: NormalizedPostRenderCaret
  selection?: NormalizedPostRenderSelection
  /** Normalized readback from the neutral Presentation Geometry API. */
  geometry?: GeometrySnapshot
}

export interface PostRenderFloatBox {
  el: HTMLElement
  /** Gapless flow top at 100% zoom, as returned by measureBlocks. */
  top?: number
  height?: number
  kind?: 'body' | 'cell' | 'header-footer'
  block?: number
  anchor?: string
}

export interface PostRenderEditorView extends PositionDiagnosticsView {
  dom: HTMLElement
  state?: {
    selection?: { anchor: number; head: number; from: number; to: number }
    doc?: GeometryProbeDocument
  }
  domAtPos?: (position: number, side?: number) => { node: Node; offset: number }
}

export interface PostRenderDiagnosticSource {
  root: HTMLElement
  flowRoot: HTMLElement
  slices: readonly PageSlice[]
  blocks?: readonly BlockBox[]
  sections?: readonly SectionInfo[]
  zoomFactor?: number
  editorView?: PostRenderEditorView
  floatBoxes?: readonly PostRenderFloatBox[]
  blockOf?: (el: HTMLElement) => number | undefined
}

function finite(value: number): number | undefined {
  return Number.isFinite(value) ? value : undefined
}

function cssPx(el: HTMLElement, name: string): number | undefined {
  const direct = parseFloat(el.style.getPropertyValue(name))
  if (Number.isFinite(direct)) return direct
  const computed = getComputedStyle(el).getPropertyValue(name)
  return finite(parseFloat(computed))
}

function rectInRoot(rect: DOMRect, rootRect: DOMRect): DiagnosticRect {
  return {
    left: rect.left - rootRect.left,
    top: rect.top - rootRect.top,
    width: rect.width,
    height: rect.height,
    right: rect.right - rootRect.left,
    bottom: rect.bottom - rootRect.top,
  }
}

function logicalRect(rect: DOMRect, flowRect: DOMRect, zoom: number): DiagnosticRect {
  return {
    left: (rect.left - flowRect.left) / zoom,
    top: (rect.top - flowRect.top) / zoom,
    width: rect.width / zoom,
    height: rect.height / zoom,
    right: (rect.right - flowRect.left) / zoom,
    bottom: (rect.bottom - flowRect.top) / zoom,
  }
}

function pageForY(y: number, pages: readonly NormalizedPostRenderPage[]): number | undefined {
  for (const page of pages) {
    const rect = page.pageRect
    if (!rect) continue
    if (y >= rect.top && y <= rect.bottom!) return page.page
    if (y < rect.top) return Math.max(1, page.page - 1)
  }
  return pages.at(-1)?.page
}

function pageRectFor(
  page: number | undefined,
  pages: readonly NormalizedPostRenderPage[],
): DiagnosticRect | undefined {
  return page == null ? undefined : pages.find((item) => item.page === page)?.pageRect
}

function sectionFor(page: number | undefined, slices: readonly PageSlice[]): number | undefined {
  return page == null ? undefined : slices[page - 1]?.section
}

function kindOfGap(el: HTMLElement): NormalizedPostRenderPageGap['kind'] {
  if (el.classList.contains('page-gap-cut')) return 'cut'
  if (el.classList.contains('page-gap-table')) return 'table'
  if (el.classList.contains('page-gap-cell')) return 'cell'
  if (el.classList.contains('page-gap-inline')) return 'inline'
  return 'block'
}

function parseShift(el: HTMLElement): number | undefined {
  return finite(parseFloat(el.dataset.pageFloatDy ?? ''))
}

function selectionRange(view: PostRenderEditorView): { from: number; to: number } | undefined {
  const selection = view.state?.selection
  if (!selection) return undefined
  return { from: selection.from, to: selection.to }
}

function selectionRects(view: PostRenderEditorView, from: number, to: number): DOMRect[] {
  const native = globalThis.getSelection?.()
  if (native && native.rangeCount > 0) {
    const range = native.getRangeAt(0)
    if (view.dom.contains(range.commonAncestorContainer))
      return [...range.getClientRects()].filter((rect) => rect.width > 0 && rect.height > 0)
  }
  if (!view.domAtPos) return []
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

function pageLocalRect(
  rect: DiagnosticRect,
  page: DiagnosticRect | undefined,
): DiagnosticRect | undefined {
  if (!page) return undefined
  return {
    left: rect.left - page.left,
    top: rect.top - page.top,
    width: rect.width,
    height: rect.height,
    right: rect.right! - page.left,
    bottom: rect.bottom! - page.top,
  }
}

function renderedFloatElements(root: HTMLElement): HTMLElement[] {
  const selectors = [
    '.doc-protected-floating > .doc-textbox',
    '.doc-img-float > .doc-img-wrap',
    '.doc-cell-boxes > .doc-textbox',
    '.doc-cell-boxes > div',
    '.page-hf-float-img',
  ]
  const seen = new Set<HTMLElement>()
  const out: HTMLElement[] = []
  for (const selector of selectors) {
    for (const el of root.querySelectorAll<HTMLElement>(selector)) {
      if (!seen.has(el)) {
        seen.add(el)
        out.push(el)
      }
    }
  }
  return out
}

/**
 * Read actual canvas geometry after App has applied its existing presentation
 * effects. This function only calls DOM read APIs and editor hit-test APIs.
 */
export function capturePostRenderDiagnostics(
  source: PostRenderDiagnosticSource,
): NormalizedPostRenderDiagnostics {
  const zoom = source.zoomFactor ?? 1
  const rootRect = source.root.getBoundingClientRect()
  const flowRect = source.flowRoot.getBoundingClientRect()
  const pageEl = source.root.querySelector<HTMLElement>(':scope > .doc-page') ?? source.flowRoot
  const pageElRect = pageEl.getBoundingClientRect()
  const pageWidth = pageElRect.width
  const pageHeightValue = cssPx(pageEl, '--page-h') ?? cssPx(pageEl, 'min-height')
  const pageHeight = pageHeightValue === undefined ? undefined : pageHeightValue * zoom
  const pageGaps: NormalizedPostRenderPageGap[] = []
  const gapElements = Array.from(
    source.root.querySelectorAll<HTMLElement>('.page-gap, .page-gap-cut'),
  )
  const gapByPage = new Map<number, NormalizedPostRenderPageGap>()

  gapElements.forEach((el, index) => {
    const rect = rectInRoot(el.getBoundingClientRect(), rootRect)
    const top = cssPx(el, '--gap-mt') ?? 0
    const bottom = cssPx(el, '--gap-mb') ?? 0
    const page = Math.min(index + 2, Math.max(1, source.slices.length))
    const bandTop = rect.top + bottom * zoom
    const bandBottom = rect.bottom! - top * zoom
    const gap: NormalizedPostRenderPageGap = {
      page,
      pageIndex: page - 1,
      boundary: { fromPage: Math.max(1, page - 1), toPage: page },
      kind: kindOfGap(el),
      pageRect: rect,
      margins: { top, bottom },
      sizePx: rect.height,
      ...(bandBottom > bandTop
        ? {
            bandRect: {
              left: rect.left,
              top: bandTop,
              width: rect.width,
              height: bandBottom - bandTop,
              right: rect.right,
              bottom: bandBottom,
            },
          }
        : {}),
      ...(source.slices[page - 1] ? { flowBoundary: source.slices[page - 1].start } : {}),
    }
    pageGaps.push(gap)
    if (!gapByPage.has(page) && gap.kind !== 'cut') gapByPage.set(page, gap)
  })

  const pages: NormalizedPostRenderPage[] = source.slices.map((slice, index) => {
    const page = index + 1
    const gap = gapByPage.get(page)
    const top =
      page === 1
        ? pageElRect.top - rootRect.top
        : gap
          ? gap.pageRect.bottom! - gap.margins.top * zoom
          : undefined
    const pageRect =
      top === undefined || pageHeight === undefined
        ? undefined
        : {
            left: pageElRect.left - rootRect.left,
            top,
            width: pageWidth,
            height: pageHeight,
            right: pageElRect.left - rootRect.left + pageWidth,
            bottom: top + pageHeight,
          }
    const flowTop = flowRect.top - rootRect.top + slice.start * zoom
    const flowLeft = flowRect.left - rootRect.left
    return {
      page,
      pageIndex: index,
      section: slice.section,
      flowRect: {
        left: 0,
        top: slice.start,
        width: flowRect.width / zoom,
        height: slice.end - slice.start,
        right: flowRect.width / zoom,
        bottom: slice.end,
      },
      ...(pageRect
        ? {
            pageRect,
            flowToPhysicalOffset: { x: pageRect.left - flowLeft, y: pageRect.top - flowTop },
          }
        : {}),
    }
  })

  const headerFooters: NormalizedPostRenderHeaderFooter[] = []
  for (const el of source.root.querySelectorAll<HTMLElement>('.page-hf')) {
    const rect = rectInRoot(el.getBoundingClientRect(), rootRect)
    const kind = el.classList.contains('page-hf-footer') ? 'footer' : 'header'
    const gap = el.closest<HTMLElement>('.page-gap')
    const gapIndex = gap ? gapElements.indexOf(gap) : -1
    const page = gap
      ? kind === 'header'
        ? Math.min(gapIndex + 2, source.slices.length)
        : Math.max(1, gapIndex + 1)
      : kind === 'header'
        ? 1
        : Math.max(1, source.slices.length)
    const variantValue = el.dataset.variant
    const variant =
      variantValue === 'first' || variantValue === 'even' || variantValue === 'default'
        ? variantValue
        : undefined
    headerFooters.push({
      page,
      pageIndex: page - 1,
      section: sectionFor(page, source.slices) ?? 0,
      kind,
      ...(variant ? { variant } : {}),
      pageRect: rect,
    })
  }

  const providedFloats = new Map<HTMLElement, PostRenderFloatBox>()
  for (const item of source.floatBoxes ?? []) providedFloats.set(item.el, item)
  const floats: NormalizedPostRenderFloat[] = []
  for (const el of renderedFloatElements(source.root)) {
    const rect = el.getBoundingClientRect()
    if (rect.width <= 0 || rect.height <= 0) continue
    const provided = providedFloats.get(el)
    const isHeaderFloat = el.classList.contains('page-hf-float-img')
    const isCellFloat = Boolean(el.closest('.doc-cell-boxes'))
    const kind = isHeaderFloat ? 'header-footer' : isCellFloat ? 'cell' : (provided?.kind ?? 'body')
    const pageRect = rectInRoot(rect, rootRect)
    const page = isHeaderFloat
      ? (() => {
          const gap = el.closest<HTMLElement>('.page-gap')
          const i = gap ? gapElements.indexOf(gap) : -1
          return gap ? Math.min(i + 2, source.slices.length) : 1
        })()
      : pageForY(pageRect.top, pages)
    const block = provided?.block ?? source.blockOf?.(el)
    const flowTop = provided?.top
    const flow =
      flowTop === undefined
        ? undefined
        : {
            left: (rect.left - flowRect.left) / zoom,
            top: flowTop,
            width: rect.width / zoom,
            height: provided?.height ?? rect.height / zoom,
            right: (rect.left - flowRect.left) / zoom + rect.width / zoom,
            bottom: flowTop + (provided?.height ?? rect.height / zoom),
          }
    const anchor =
      el.closest<HTMLElement>('[data-idx]')?.getAttribute('data-idx') ??
      el.closest<HTMLElement>('[data-docx-index]')?.getAttribute('data-docx-index') ??
      provided?.anchor
    floats.push({
      ...(page !== undefined ? { page, section: sectionFor(page, source.slices) } : {}),
      ...(page !== undefined ? { pageIndex: page - 1 } : {}),
      kind,
      ...(block !== undefined ? { block } : {}),
      ...(anchor ? { anchor } : {}),
      ...(flow ? { flowRect: flow } : {}),
      pageRect,
      ...(parseShift(el) !== undefined ? { domShiftY: parseShift(el) } : {}),
    })
  }

  let caret: NormalizedPostRenderCaret | undefined
  let selection: NormalizedPostRenderSelection | undefined
  const range = source.editorView ? selectionRange(source.editorView) : undefined
  if (source.editorView && range) {
    const pageForRect = (rect: DiagnosticRect) => pageForY(rect.top, pages)
    if (range.from === range.to) {
      const position = range.from
      try {
        const rect = source.editorView.coordsAtPos(position, 1)
        const pageRect = rectInRoot(rect as DOMRect, rootRect)
        const page = pageForRect(pageRect)
        const physicalPage = pageRectFor(page, pages)
        caret = {
          position,
          ...(page !== undefined ? { page, section: sectionFor(page, source.slices) } : {}),
          ...(page !== undefined ? { pageIndex: page - 1 } : {}),
          flowRect: logicalRect(rect as DOMRect, flowRect, zoom),
          pageRect,
          ...(pageLocalRect(pageRect, physicalPage)
            ? { pageLocalRect: pageLocalRect(pageRect, physicalPage) }
            : {}),
        }
      } catch {
        caret = { position }
      }
    } else {
      const rects = selectionRects(source.editorView, range.from, range.to)
      const normalizedRects = rects.map((rect) => {
        const pageRect = rectInRoot(rect, rootRect)
        const page = pageForRect(pageRect)
        const physicalPage = pageRectFor(page, pages)
        return {
          ...(page !== undefined ? { page, section: sectionFor(page, source.slices) } : {}),
          ...(page !== undefined ? { pageIndex: page - 1 } : {}),
          flowRect: logicalRect(rect, flowRect, zoom),
          pageRect,
          ...(pageLocalRect(pageRect, physicalPage)
            ? { pageLocalRect: pageLocalRect(pageRect, physicalPage) }
            : {}),
        }
      })
      const pageSet = [
        ...new Set(
          normalizedRects.map((item) => item.page).filter((p): p is number => p !== undefined),
        ),
      ]
      selection = {
        pmRange: range,
        pages: pageSet,
        pageIndexes: pageSet.map((page) => page - 1),
        sections: [
          ...new Set(
            normalizedRects.map((item) => item.section).filter((s): s is number => s !== undefined),
          ),
        ],
        ...(normalizedRects.length > 0 ? { rects: normalizedRects } : {}),
      }
    }
  }

  const geometry = snapshotPresentationGeometry(
    createPresentationGeometry({
      root: source.root,
      flowRoot: source.flowRoot,
      slices: source.slices,
      blocks: source.blocks,
      sections: source.sections,
      editorView: source.editorView,
      zoomFactor: source.zoomFactor,
    }),
    {
      positions: range ? (range.from === range.to ? [range.from] : [range.from, range.to]) : [],
      selections: range ? [range] : [],
    },
  )

  return {
    coordinateSpaces: {
      document: 'page-wrap-relative-css-px',
      flow: 'gapless-layout-px-at-100-percent',
      viewport: 'discarded-after-normalization',
      pageIndex: 'zero-based',
      pageNumber: 'one-based-legacy-diagnostic',
      zoomFactor: zoom,
    },
    pages,
    pageGaps,
    headerFooters,
    floats,
    ...(caret ? { caret } : {}),
    ...(selection ? { selection } : {}),
    geometry,
  }
}

/** Read-only probe capture for tests and debug tooling; never mutates editor or DOM state. */
export function captureGeometryProbeDiagnostics(
  source: PostRenderDiagnosticSource,
  probes: readonly GeometryProbe[],
): GeometryProbeResult[] {
  const geometry = createPresentationGeometry({
    root: source.root,
    flowRoot: source.flowRoot,
    slices: source.slices,
    blocks: source.blocks,
    sections: source.sections,
    editorView: source.editorView,
    zoomFactor: source.zoomFactor,
  })
  const pointResolver: GeometryProbePointResolver | undefined = source.editorView
    ? (pmPosition) => {
        try {
          const rect = source.editorView!.coordsAtPos(pmPosition, 1)
          return [{ space: 'viewport', x: rect.left, y: rect.top }]
        } catch {
          return []
        }
      }
    : undefined
  return capturePresentationGeometryProbes(
    geometry,
    source.editorView?.state?.doc,
    probes,
    pointResolver,
  )
}
