import type { SectionInfo } from '@genoffice/docx-engine'
import { isDocDirty, type DocDirtyState } from '../doc-dirty'
import type { PmNode } from '../editor/convert'
import { GAP_BAND, type GapMetrics, type PageGapSpec } from '../editor/pagination-gaps'
import {
  columnLayoutSpecs,
  pageAt,
  pageStartBlocks,
  sectionPageBox,
  vAlignShiftSpecs,
  type BlockBox,
  type PageSlice,
  type SectionGeom,
} from '../pagination'
import type { NormalizedPostRenderDiagnostics } from './post-render'

/**
 * Diagnostics-only geometry tolerance. It is intentionally much smaller than a
 * line height or page-break decision, so a real placement change still fails.
 */
export const PRESENTATION_GEOMETRY_TOLERANCE_PX = 0.01

export type DiagnosticCategory =
  | 'page'
  | 'page-physical'
  | 'page-gap'
  | 'line'
  | 'table'
  | 'column'
  | 'header-footer'
  | 'float'
  | 'caret'
  | 'selection'
  | 'coordinate-mapping'
  | 'mapping'
  | 'model'
  | 'dirty'
  | 'save'

export interface DiagnosticRect {
  left: number
  top: number
  width: number
  height: number
  right?: number
  bottom?: number
}

export interface DiagnosticRange {
  from: number
  to: number
}

export interface NormalizedPageSlice {
  page: number
  start: number
  end: number
  section: number
  repeatHeader?: { top: number; height: number }
  regions?: Array<{
    top: number
    height: number
    section: number
    columns: Array<{ start: number; end: number; repeatHeader?: { top: number; height: number } }>
  }>
  physHeight?: number
}

export interface NormalizedSectionGeometry extends SectionGeom {
  section: number
  firstPage?: number
  flowOffsetY?: number
  pageBox?: {
    width: number
    height: number
    contentWidth: number
    headerDist: number
    footerDist: number
  }
}

export interface NormalizedPageGeometry {
  page: number
  section: number
  flowStart: number
  flowEnd: number
  physicalOffsetY?: number
  width?: number
  height?: number
  contentWidth?: number
  headerDist?: number
  footerDist?: number
}

export interface NormalizedLineBox {
  block: number
  line: number
  offsetInBlock: number
  top: number
  height: number
  page: number
  pmRange?: DiagnosticRange
}

export interface NormalizedTableGeometry {
  block: number
  rows: Array<{
    height: number
    cantSplit?: boolean
    isHeader?: boolean
    vMergeContinue?: boolean
    cutYs?: number[]
    contentBottom?: number
  }>
}

export interface NormalizedColumnPlacement {
  block: number
  widthPx?: number
  dx: number
  dy: number
  kind: 'column' | 'vAlign'
}

export interface PageGapDiagnosticInput {
  page?: number
  block?: number
  pos?: number
  kind?: 'block' | 'inline' | 'table' | 'cut' | 'cell'
  metrics: GapMetrics
  pullUp?: number
  suppressLeadMt?: boolean
  hasNotes?: boolean
  hasHeaderFooter?: boolean
  hasRepeatedHeader?: boolean
}

export interface NormalizedPageGap extends PageGapDiagnosticInput {
  kind: NonNullable<PageGapDiagnosticInput['kind']>
  height: number
}

export interface HeaderFooterDiagnosticInput {
  page: number
  kind: 'header' | 'footer'
  variant?: 'default' | 'first' | 'even'
  rect?: DiagnosticRect
  height?: number
  source?: string
}

export interface NormalizedHeaderFooterPlacement extends HeaderFooterDiagnosticInput {}

export interface FloatShiftDiagnosticInput {
  page?: number
  block?: number
  dx?: number
  dy: number
  rect?: DiagnosticRect
}

export interface NormalizedFloatShift extends FloatShiftDiagnosticInput {}

export interface NotePlacementDiagnosticInput {
  page: number
  kind: 'footnote' | 'endnote'
  id?: number
  top?: number
  height?: number
}

export interface NormalizedPositionMapping {
  position: number
  rect?: DiagnosticRect
  hitPosition?: number
  roundTrip?: boolean
}

export interface PresentationDiagnosticSource {
  blocks: readonly BlockBox[]
  sectionGeoms: readonly SectionGeom[]
  slices: readonly PageSlice[]
  sections?: readonly SectionInfo[]
  /** Physical page offsets are only included when the caller has measured them. */
  pageOffsetsY?: readonly number[]
  blockPmRangeOf?: (block: BlockBox, blockIndex: number) => DiagnosticRange | undefined
  pageGaps?: readonly PageGapDiagnosticInput[]
  headerFooters?: readonly HeaderFooterDiagnosticInput[]
  floatShifts?: readonly FloatShiftDiagnosticInput[]
  notes?: readonly NotePlacementDiagnosticInput[]
  positionMappings?: readonly NormalizedPositionMapping[]
  caret?: DiagnosticRect
  selection?: DiagnosticRect[]
  /** Read-only geometry captured after live DOM presentation effects. */
  postRender?: NormalizedPostRenderDiagnostics
}

export interface NormalizedPresentationDiagnostics {
  pageCount: number
  slices: NormalizedPageSlice[]
  sections: NormalizedSectionGeometry[]
  pages: NormalizedPageGeometry[]
  lines: NormalizedLineBox[]
  tables: NormalizedTableGeometry[]
  pageStarts: Array<{ page: number; block: number }>
  columns: NormalizedColumnPlacement[]
  pageGaps: NormalizedPageGap[]
  headerFooters: NormalizedHeaderFooterPlacement[]
  floats: NormalizedFloatShift[]
  notes: NotePlacementDiagnosticInput[]
  positionMappings: NormalizedPositionMapping[]
  caret?: DiagnosticRect
  selection?: DiagnosticRect[]
  postRender?: NormalizedPostRenderDiagnostics
}

export interface ModelDiagnosticInput {
  pmJson: PmNode | unknown
  selection?: { anchor: number; head: number; from: number; to: number }
  dirtyState: DocDirtyState
  savePlan: unknown
  savedBytes?: ArrayLike<number>
  /** Use only when raw ZIP bytes contain known producer metadata differences. */
  normalizedSavedParts?: Record<string, unknown>
  reopenedPmJson: PmNode | unknown
  reopenedSelection?: { anchor: number; head: number; from: number; to: number }
}

export interface NormalizedModelDiagnostics {
  pmJson: unknown
  selection?: unknown
  dirty: boolean
  dirtyState: unknown
  savePlan: unknown
  saveOutput?: { kind: 'bytes'; bytes: number[] } | { kind: 'parts'; parts: unknown }
  reopenedPmJson: unknown
  reopenedSelection?: unknown
}

export interface DiagnosticParityDifference {
  fixture?: string
  renderer?: string
  category: DiagnosticCategory
  path: string
  page?: number
  block?: number
  pmRange?: DiagnosticRange
  coordinateSpace?: string
  expected: unknown
  actual: unknown
  delta?: number
}

export interface DiagnosticParityOptions {
  fixture?: string
  renderer?: string
  geometryTolerancePx?: number
  maxDifferences?: number
}

export interface PositionDiagnosticsView {
  coordsAtPos(
    position: number,
    side?: number,
  ): {
    left: number
    top: number
    right: number
    bottom: number
    width?: number
    height?: number
  }
  posAtCoords(coords: { left: number; top: number }): { pos: number } | null
}

function numberOr(value: number | undefined, fallback: number): number {
  return Number.isFinite(value) ? (value as number) : fallback
}

function rectOf(rect: DiagnosticRect | undefined): DiagnosticRect | undefined {
  if (!rect) return undefined
  return {
    left: rect.left,
    top: rect.top,
    width: rect.width,
    height: rect.height,
    ...(rect.right !== undefined ? { right: rect.right } : {}),
    ...(rect.bottom !== undefined ? { bottom: rect.bottom } : {}),
  }
}

function blockKey(block: BlockBox, index: number): number {
  return block.docxIndex ?? index
}

function rangeOf(value: { from: number; to: number } | undefined): DiagnosticRange | undefined {
  return value ? { from: value.from, to: value.to } : undefined
}

function normalizeSlice(slice: PageSlice, pageIndex: number): NormalizedPageSlice {
  return {
    page: pageIndex + 1,
    start: slice.start,
    end: slice.end,
    section: slice.section,
    ...(slice.repeatHeader ? { repeatHeader: { ...slice.repeatHeader } } : {}),
    ...(slice.regions
      ? {
          regions: slice.regions.map((region) => ({
            top: region.top,
            height: region.height,
            section: region.section,
            columns: region.columns.map((column) => ({
              start: column.start,
              end: column.end,
              ...(column.repeatHeader ? { repeatHeader: { ...column.repeatHeader } } : {}),
            })),
          })),
        }
      : {}),
    ...(slice.physHeight !== undefined ? { physHeight: slice.physHeight } : {}),
  }
}

function normalizeSection(
  geom: SectionGeom,
  section: number,
  firstPage: number | undefined,
  flowOffsetY: number | undefined,
  settings: SectionInfo['settings'] | undefined,
): NormalizedSectionGeometry {
  return {
    section,
    contentHeight: geom.contentHeight,
    forceBreak: geom.forceBreak,
    ...(geom.startType !== undefined ? { startType: geom.startType } : {}),
    ...(geom.cols !== undefined ? { cols: geom.cols } : {}),
    ...(geom.colWidths ? { colWidths: [...geom.colWidths] } : {}),
    ...(geom.colBreakStart !== undefined ? { colBreakStart: geom.colBreakStart } : {}),
    ...(firstPage !== undefined ? { firstPage } : {}),
    ...(flowOffsetY !== undefined ? { flowOffsetY } : {}),
    ...(settings ? { pageBox: sectionPageBox(settings) } : {}),
  }
}

function normalizePage(
  slice: PageSlice,
  page: number,
  settings: SectionInfo['settings'] | undefined,
  physicalOffsetY: number | undefined,
): NormalizedPageGeometry {
  const box = settings ? sectionPageBox(settings) : undefined
  return {
    page,
    section: slice.section,
    flowStart: slice.start,
    flowEnd: slice.end,
    ...(physicalOffsetY !== undefined ? { physicalOffsetY } : {}),
    ...(box
      ? {
          width: box.width,
          height: box.height,
          contentWidth: box.contentWidth,
          headerDist: box.headerDist,
          footerDist: box.footerDist,
        }
      : {}),
  }
}

function normalizePageGap(input: PageGapDiagnosticInput): NormalizedPageGap {
  const kind = input.kind ?? 'block'
  return {
    ...input,
    kind,
    metrics: { ...input.metrics },
    height: kind === 'cut' ? 0 : input.metrics.marginBottom + GAP_BAND + input.metrics.marginTop,
  }
}

/** Convert existing page-gap side effects into serializable diagnostics, dropping DOM identity and runtime keys. */
export function pageGapDiagnosticsFromSpecs(
  specs: readonly PageGapSpec[],
): PageGapDiagnosticInput[] {
  return specs.map((spec) => ({
    ...('pos' in spec && typeof spec.pos === 'number' ? { pos: spec.pos } : {}),
    kind: 'el' in spec ? 'block' : spec.kind,
    metrics: { ...spec.metrics },
    ...(spec.pullUp !== undefined ? { pullUp: spec.pullUp } : {}),
    ...(spec.suppressLeadMt ? { suppressLeadMt: true } : {}),
    ...(spec.notesKey ? { hasNotes: true } : {}),
    ...(spec.hfKey ? { hasHeaderFooter: true } : {}),
    ...(spec.repeatHeaderKey ? { hasRepeatedHeader: true } : {}),
  }))
}

/**
 * Capture one renderer's current presentation side effects. This function only
 * reads existing inputs/outputs; it never mutates PM, DOM, decorations, or save state.
 */
export function capturePresentationDiagnostics(
  source: PresentationDiagnosticSource,
): NormalizedPresentationDiagnostics {
  const slices = source.slices.map(normalizeSlice)
  const firstPages = new Map<number, number>()
  for (const [pageIndex, slice] of source.slices.entries()) {
    if (!firstPages.has(slice.section)) firstPages.set(slice.section, pageIndex + 1)
  }
  const firstFlow = new Map<number, number>()
  for (const slice of source.slices) {
    if (!firstFlow.has(slice.section)) firstFlow.set(slice.section, slice.start)
  }

  const sections = source.sectionGeoms.map((geom, index) =>
    normalizeSection(
      geom,
      index,
      firstPages.get(index),
      firstFlow.get(index),
      source.sections?.[index]?.settings,
    ),
  )
  const pages = source.slices.map((slice, index) =>
    normalizePage(
      slice,
      index + 1,
      source.sections?.[slice.section]?.settings,
      source.pageOffsetsY?.[index],
    ),
  )

  const lines: NormalizedLineBox[] = []
  const tables: NormalizedTableGeometry[] = []
  for (const [index, block] of source.blocks.entries()) {
    const key = blockKey(block, index)
    const pmRange = rangeOf(source.blockPmRangeOf?.(block, index))
    for (const [line, lineBox] of (block.lineBoxes ?? []).entries()) {
      const top = block.top + lineBox.offsetInBlock
      lines.push({
        block: key,
        line,
        offsetInBlock: lineBox.offsetInBlock,
        top,
        height: lineBox.height,
        page: pageAt([...source.slices], top + 0.001),
        ...(pmRange ? { pmRange } : {}),
      })
    }
    if (block.tableRows) {
      tables.push({
        block: key,
        rows: block.tableRows.map((row) => ({
          height: row.height,
          ...(row.cantSplit !== undefined ? { cantSplit: row.cantSplit } : {}),
          ...(row.isHeader !== undefined ? { isHeader: row.isHeader } : {}),
          ...(row.vMergeContinue !== undefined ? { vMergeContinue: row.vMergeContinue } : {}),
          ...(row.cutYs ? { cutYs: [...row.cutYs] } : {}),
          ...(row.contentBottom !== undefined ? { contentBottom: row.contentBottom } : {}),
        })),
      })
    }
  }

  const columns: NormalizedColumnPlacement[] = []
  if (source.sections) {
    const blockOf = (el: HTMLElement) => source.blocks.findIndex((block) => block.el === el)
    for (const placement of columnLayoutSpecs(
      [...source.blocks],
      [...source.slices],
      [...source.sections],
    )) {
      const index = blockOf(placement.el)
      if (index >= 0)
        columns.push({
          block: blockKey(source.blocks[index], index),
          ...(placement.widthPx !== undefined ? { widthPx: placement.widthPx } : {}),
          dx: placement.dx,
          dy: placement.dy,
          kind: 'column',
        })
    }
    for (const placement of vAlignShiftSpecs(
      [...source.blocks],
      [...source.slices],
      [...source.sections],
      [...source.sectionGeoms],
    )) {
      const index = blockOf(placement.el)
      if (index >= 0)
        columns.push({
          block: blockKey(source.blocks[index], index),
          dx: placement.dx,
          dy: placement.dy,
          kind: 'vAlign',
        })
    }
  }

  const starts = pageStartBlocks([...source.blocks], [...source.slices])
  const pageStarts = starts.map((index, page) => ({
    page: page + 1,
    block: blockKey(source.blocks[index], index),
  }))

  return {
    pageCount: source.slices.length,
    slices,
    sections,
    pages,
    lines,
    tables,
    pageStarts,
    columns,
    pageGaps: (source.pageGaps ?? []).map(normalizePageGap),
    headerFooters: (source.headerFooters ?? []).map((item) => ({
      ...item,
      ...(item.rect ? { rect: rectOf(item.rect) } : {}),
    })),
    floats: (source.floatShifts ?? []).map((item) => ({
      ...item,
      ...(item.rect ? { rect: rectOf(item.rect) } : {}),
    })),
    notes: (source.notes ?? []).map((item) => ({ ...item })),
    positionMappings: (source.positionMappings ?? []).map((item) => ({
      ...item,
      ...(item.rect ? { rect: rectOf(item.rect) } : {}),
    })),
    ...(source.caret ? { caret: rectOf(source.caret) } : {}),
    ...(source.selection ? { selection: source.selection.map((rect) => rectOf(rect)!) } : {}),
    ...(source.postRender ? { postRender: source.postRender } : {}),
  }
}

/** Capture the position/caret hit-test APIs already exposed by ProseMirror's EditorView. */
export function captureEditorPositionDiagnostics(
  view: PositionDiagnosticsView,
  positions: readonly number[],
): NormalizedPositionMapping[] {
  return positions.map((position) => {
    try {
      const rect = view.coordsAtPos(position, 1)
      const hit = view.posAtCoords({ left: rect.left, top: rect.top })
      return {
        position,
        rect: {
          left: rect.left,
          top: rect.top,
          right: rect.right,
          bottom: rect.bottom,
          width: numberOr(rect.width, rect.right - rect.left),
          height: numberOr(rect.height, rect.bottom - rect.top),
        },
        ...(hit ? { hitPosition: hit.pos, roundTrip: hit.pos === position } : {}),
      }
    } catch {
      return { position }
    }
  })
}

/** Normalize runtime containers while preserving every meaningful value in the diagnostic model. */
export function normalizeDiagnosticValue(value: unknown): unknown {
  if (value === undefined) return undefined
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return value
  if (typeof value === 'number') return value
  if (typeof value === 'bigint') return `${value}n`
  if (value instanceof Uint8Array) return Array.from(value)
  if (Array.isArray(value)) return value.map(normalizeDiagnosticValue)
  if (value instanceof Map) {
    return [...value.entries()]
      .map(([key, item]) => [normalizeDiagnosticValue(key), normalizeDiagnosticValue(item)])
      .sort(([a], [b]) => JSON.stringify(a).localeCompare(JSON.stringify(b)))
  }
  if (value instanceof Set) {
    return [...value]
      .map(normalizeDiagnosticValue)
      .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)))
  }
  if (typeof value === 'object') {
    const out: Record<string, unknown> = {}
    for (const key of Object.keys(value as Record<string, unknown>).sort()) {
      const normalized = normalizeDiagnosticValue((value as Record<string, unknown>)[key])
      if (normalized !== undefined) out[key] = normalized
    }
    return out
  }
  return String(value)
}

export function captureModelDiagnostics(input: ModelDiagnosticInput): NormalizedModelDiagnostics {
  return {
    pmJson: normalizeDiagnosticValue(input.pmJson),
    ...(input.selection ? { selection: normalizeDiagnosticValue(input.selection) } : {}),
    dirty: isDocDirty(input.dirtyState),
    dirtyState: normalizeDiagnosticValue(input.dirtyState),
    savePlan: normalizeDiagnosticValue(input.savePlan),
    ...(input.savedBytes
      ? { saveOutput: { kind: 'bytes', bytes: Array.from(input.savedBytes as ArrayLike<number>) } }
      : input.normalizedSavedParts
        ? {
            saveOutput: {
              kind: 'parts',
              parts: normalizeDiagnosticValue(input.normalizedSavedParts),
            },
          }
        : {}),
    reopenedPmJson: normalizeDiagnosticValue(input.reopenedPmJson),
    ...(input.reopenedSelection
      ? { reopenedSelection: normalizeDiagnosticValue(input.reopenedSelection) }
      : {}),
  }
}

function categoryForPath(path: string): DiagnosticCategory {
  if (/dirty/i.test(path)) return 'dirty'
  if (/save|reopen|savePlan|saveOutput/i.test(path)) return 'save'
  if (/postRender.*pageGaps|pageGaps|bandRect|sizePx/i.test(path)) return 'page-gap'
  if (/postRender.*pages.*pageRect|pagePhysical|physicalPage/i.test(path)) return 'page-physical'
  if (/postRender.*headerFooters/i.test(path)) return 'header-footer'
  if (/postRender.*floats/i.test(path)) return 'float'
  if (/postRender.*caret/i.test(path)) return 'caret'
  if (/postRender.*selection/i.test(path)) return 'selection'
  if (/flowToPhysical|flowRect|pageLocalRect|coordinateSpace/i.test(path))
    return 'coordinate-mapping'
  if (/pmJson|selection|reopened/i.test(path)) return /selection/i.test(path) ? 'mapping' : 'model'
  if (/line/i.test(path)) return 'line'
  if (/table/i.test(path)) return 'table'
  if (/column/i.test(path)) return 'column'
  if (/header|footer|note/i.test(path)) return 'header-footer'
  if (/float/i.test(path)) return 'float'
  if (/mapping|caret|position/i.test(path)) return 'mapping'
  return 'page'
}

function contextOf(
  value: unknown,
  parent: { page?: number; block?: number; pmRange?: DiagnosticRange },
) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return parent
  const object = value as Record<string, unknown>
  return {
    page: typeof object.page === 'number' ? object.page : parent.page,
    block: typeof object.block === 'number' ? object.block : parent.block,
    pmRange:
      object.pmRange && typeof object.pmRange === 'object'
        ? (object.pmRange as DiagnosticRange)
        : parent.pmRange,
  }
}

function isGeometryPath(path: string): boolean {
  return /(?:\.top|\.bottom|\.left|\.right|\.width|\.height|\.dx|\.dy|OffsetY|flowStart|flowEnd|offsetInBlock|cutYs|contentBottom|physHeight|sizePx|flowToPhysical|pageRect|flowRect|pageLocalRect)/i.test(
    path,
  )
}

function coordinateSpaceForPath(path: string): string | undefined {
  if (/viewport/i.test(path)) return 'viewport'
  if (/flowRect|flowStart|flowEnd|flowBoundary/i.test(path)) return 'flow'
  if (/pageRect|pageLocalRect|bandRect|physical/i.test(path)) return 'page-wrap'
  if (/flowToPhysical|coordinate/i.test(path)) return 'mapping'
  return undefined
}

function display(value: unknown): string {
  const text = JSON.stringify(value)
  return text.length > 240 ? `${text.slice(0, 237)}...` : text
}

/** Compare normalized diagnostics and return compact, location-aware differences. */
export function compareDiagnosticParity(
  expected: unknown,
  actual: unknown,
  options: DiagnosticParityOptions = {},
): DiagnosticParityDifference[] {
  const differences: DiagnosticParityDifference[] = []
  const max = options.maxDifferences ?? 50
  const tolerance = options.geometryTolerancePx ?? PRESENTATION_GEOMETRY_TOLERANCE_PX

  const visit = (
    left: unknown,
    right: unknown,
    path: string,
    parent: { page?: number; block?: number; pmRange?: DiagnosticRange },
  ): void => {
    if (differences.length >= max) return
    const context = contextOf(right ?? left, parent)
    if (typeof left === 'number' && typeof right === 'number') {
      const delta = right - left
      if (isGeometryPath(path) && Math.abs(delta) <= tolerance) return
      if (Object.is(left, right)) return
      differences.push({
        ...(options.fixture ? { fixture: options.fixture } : {}),
        ...(options.renderer ? { renderer: options.renderer } : {}),
        category: categoryForPath(path),
        path,
        ...context,
        ...(coordinateSpaceForPath(path)
          ? { coordinateSpace: coordinateSpaceForPath(path) }
          : {}),
        expected: left,
        actual: right,
        delta,
      })
      return
    }
    if (Object.is(left, right)) return
    if (Array.isArray(left) && Array.isArray(right)) {
      const length = Math.max(left.length, right.length)
      for (let i = 0; i < length; i++) visit(left[i], right[i], `${path}[${i}]`, context)
      return
    }
    if (left && right && typeof left === 'object' && typeof right === 'object') {
      const keys = new Set([...Object.keys(left), ...Object.keys(right)])
      for (const key of [...keys].sort())
        visit(
          (left as Record<string, unknown>)[key],
          (right as Record<string, unknown>)[key],
          path ? `${path}.${key}` : key,
          context,
        )
      return
    }
    differences.push({
      ...(options.fixture ? { fixture: options.fixture } : {}),
      ...(options.renderer ? { renderer: options.renderer } : {}),
      category: categoryForPath(path),
      path,
      ...context,
      ...(coordinateSpaceForPath(path)
        ? { coordinateSpace: coordinateSpaceForPath(path) }
        : {}),
      expected: left,
      actual: right,
    })
  }

  visit(normalizeDiagnosticValue(expected), normalizeDiagnosticValue(actual), '', {})
  return differences
}

export function formatDiagnosticDiffs(differences: readonly DiagnosticParityDifference[]): string {
  if (differences.length === 0) return 'presentation parity: equal'
  return differences
    .map((difference) => {
      const location = [
        difference.fixture ? `fixture=${difference.fixture}` : undefined,
        difference.renderer ? `renderer=${difference.renderer}` : undefined,
        `category=${difference.category}`,
        difference.page !== undefined ? `page=${difference.page}` : undefined,
        difference.block !== undefined ? `block=${difference.block}` : undefined,
        difference.pmRange ? `pm=${difference.pmRange.from}-${difference.pmRange.to}` : undefined,
        difference.coordinateSpace ? `space=${difference.coordinateSpace}` : undefined,
      ]
        .filter(Boolean)
        .join(' ')
      const delta = difference.delta !== undefined ? ` delta=${difference.delta}` : ''
      return `${location} path=${difference.path} expected=${display(difference.expected)} actual=${display(difference.actual)}${delta}`
    })
    .join('\n')
}
