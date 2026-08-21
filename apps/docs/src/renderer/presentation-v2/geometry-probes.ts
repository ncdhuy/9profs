import {
  pageNumberFromPageIndex,
  type DiagnosticCategory,
  type DiagnosticParityDifference,
} from './diagnostics'
import {
  type GeometryPoint,
  type PointToPositionResult,
  type PositionGeometry,
  type PresentationGeometry,
  type SelectionGeometry,
} from './geometry'

export type GeometryProbeSemanticCase =
  | 'paragraph-middle'
  | 'line-start'
  | 'line-end'
  | 'page-before-gap'
  | 'page-after-gap'
  | 'table-cell'
  | 'table-row-boundary'
  | 'repeated-header'
  | 'nested-table'
  | 'column-1'
  | 'column-2'
  | 'column-transition'
  | 'header'
  | 'footer'
  | 'floating-object-anchor'
  | 'textbox'
  | 'cjk-run'
  | 'revision-range'
  | 'comment-range'

export type GeometryProbeOffset = 'start' | 'middle' | 'end'

export interface GeometryProbeNodeLike {
  type?: string | { name?: string }
  attrs?: Record<string, unknown>
  text?: string
  textContent?: string
  marks?: ReadonlyArray<{ type: string | { name?: string }; attrs?: Record<string, unknown> }>
  nodeSize?: number
  childCount?: number
  child?: (index: number) => GeometryProbeNodeLike
  content?:
    | GeometryProbeNodeLike[]
    | {
        childCount?: number
        child?: (index: number) => GeometryProbeNodeLike
      }
}

export type GeometryProbeDocument = GeometryProbeNodeLike

interface NodeLocation {
  node: GeometryProbeNodeLike
  nodeStart: number
  contentStart: number
  contentEnd: number
  parent?: NodeLocation
  index?: number
}

interface ResolvedProbeAnchor {
  position?: number
  range?: { from: number; to: number }
  context?: GeometryProbeContext
  reason?: GeometryProbeUnavailableReason
}

export interface GeometryProbeContext {
  nodeType?: string
  docxIndex?: number
  textScript?: 'cjk' | 'latin' | 'mixed' | 'unknown'
  table?: { docxIndex?: number; row: number; cell: number }
  mark?: { type: string; id?: string }
}

export interface GeometryProbeExpectation {
  pageIndex?: number
  sectionIndex?: number
  columnIndex?: number
  nodeType?: string
  docxIndex?: number
  textScript?: GeometryProbeContext['textScript']
  markType?: string
  table?: { row: number; cell: number }
}

interface NodeProbeAnchor {
  kind: 'node'
  nodeType: string
  occurrence?: number
  docxIndex?: number
  attrs?: Record<string, unknown>
  offset?: GeometryProbeOffset
}

interface BlockProbeAnchor {
  kind: 'block'
  docxIndex: number
  offset?: GeometryProbeOffset
}

interface TextProbeAnchor {
  kind: 'text'
  text: string
  occurrence?: number
  nodeType?: string
  docxIndex?: number
  offset?: GeometryProbeOffset | number
}

interface TableCellProbeAnchor {
  kind: 'table-cell'
  tableOccurrence?: number
  tableDocxIndex?: number
  row: number | 'last'
  cell: number | 'last'
  offset?: GeometryProbeOffset
}

interface TableRowProbeAnchor {
  kind: 'table-row-boundary'
  tableOccurrence?: number
  tableDocxIndex?: number
  row: number | 'last'
  offset?: GeometryProbeOffset
}

interface MarkRangeProbeAnchor {
  kind: 'mark-range'
  markType: 'comment' | 'ins' | 'del'
  occurrence?: number
  id?: string
}

interface PageBoundaryProbeAnchor {
  kind: 'page-boundary'
  pageIndex: number
  side: 'before-gap' | 'after-gap'
}

interface ColumnProbeAnchor {
  kind: 'column'
  pageIndex?: number
  columnIndex: number
  side?: 'first' | 'last'
}

interface ColumnTransitionProbeAnchor {
  kind: 'column-transition'
  pageIndex?: number
  fromColumn: number
  toColumn: number
}

interface PresentationOnlyProbeAnchor {
  kind: 'header-footer'
  part: 'header' | 'footer'
}

export type GeometryProbeAnchor =
  | NodeProbeAnchor
  | BlockProbeAnchor
  | TextProbeAnchor
  | TableCellProbeAnchor
  | TableRowProbeAnchor
  | MarkRangeProbeAnchor
  | PageBoundaryProbeAnchor
  | ColumnProbeAnchor
  | ColumnTransitionProbeAnchor
  | PresentationOnlyProbeAnchor

export interface GeometryProbe {
  id: string
  fixtureId: string
  semanticCase: GeometryProbeSemanticCase
  anchor: GeometryProbeAnchor
  expected?: GeometryProbeExpectation
  /** Optional cases remain visible as unavailable diagnostics when PM state cannot expose them. */
  optional?: boolean
}

export type GeometryProbeUnavailableReason =
  | 'anchor-not-found'
  | 'presentation-only-no-pm-position'
  | 'page-boundary-not-found'
  | 'column-not-found'
  | 'column-transition-not-found'
  | 'mark-not-found'

export interface GeometryProbeRoundTrip {
  status: 'exact' | 'boundary-ambiguous' | 'mismatch' | 'unavailable'
  expectedPosition: number
  actualPosition?: number
  delta?: number
  reason?: string
}

export interface GeometryProbeResult {
  probe: GeometryProbe
  status: 'resolved' | 'unavailable'
  mappingStatus: 'resolved' | 'boundary-ambiguous' | 'unavailable'
  reason?: GeometryProbeUnavailableReason | 'editor-coordinates-unavailable'
  pmPosition?: number
  pmRange?: { from: number; to: number }
  pageIndex?: number
  pageNumber?: number
  sectionIndex?: number
  structuralContext?: GeometryProbeContext
  positionGeometry?: PositionGeometry
  selectionGeometry?: SelectionGeometry
  stablePoint?: GeometryPoint
  reverse?: PointToPositionResult
  roundTrip?: GeometryProbeRoundTrip
}

export type GeometryProbePointResolver = (
  pmPosition: number,
  position: PositionGeometry,
) => readonly GeometryPoint[]

function typeName(node: GeometryProbeNodeLike): string {
  return typeof node.type === 'string' ? node.type : (node.type?.name ?? '')
}

function markTypeName(mark: { type: string | { name?: string } }): string {
  return typeof mark.type === 'string' ? mark.type : (mark.type.name ?? '')
}

function children(node: GeometryProbeNodeLike): GeometryProbeNodeLike[] {
  if (Array.isArray(node.content)) return node.content
  const content = node.content
  if (!content?.child || !Number.isInteger(content.childCount)) return []
  const count = content.childCount ?? 0
  return Array.from({ length: count }, (_, index) => content.child!(index))
}

function nodeSize(node: GeometryProbeNodeLike): number {
  if (typeof node.nodeSize === 'number') return node.nodeSize
  if (typeof node.text === 'string') return node.text.length
  const nested = children(node)
  return nested.length > 0 ? nested.reduce((sum, child) => sum + nodeSize(child), 2) : 1
}

function textContent(node: GeometryProbeNodeLike): string {
  if (typeof node.text === 'string') return node.text
  if (typeof node.textContent === 'string') return node.textContent
  return children(node).map(textContent).join('')
}

function scriptOf(text: string): GeometryProbeContext['textScript'] {
  const hasCjk = /[\u2e80-\u9fff\uf900-\ufaff\uff00-\uffef]/u.test(text)
  const hasLatin = /[A-Za-z]/u.test(text)
  if (hasCjk && hasLatin) return 'mixed'
  if (hasCjk) return 'cjk'
  if (hasLatin) return 'latin'
  return 'unknown'
}

function walk(
  node: GeometryProbeNodeLike,
  nodeStart: number,
  parent: NodeLocation | undefined,
  index: number | undefined,
  visit: (location: NodeLocation) => void,
): void {
  const textNode = typeof node.text === 'string'
  const size = nodeSize(node)
  const location: NodeLocation = {
    node,
    nodeStart,
    contentStart: textNode ? nodeStart : typeName(node) === 'doc' ? 0 : nodeStart + 1,
    contentEnd: textNode
      ? nodeStart + size
      : typeName(node) === 'doc'
        ? size
        : nodeStart + size - 1,
    parent,
    index,
  }
  visit(location)
  if (textNode) return
  let childStart = location.contentStart
  for (const [childIndex, child] of children(node).entries()) {
    walk(child, childStart, location, childIndex, visit)
    childStart += nodeSize(child)
  }
}

function locations(doc: GeometryProbeDocument): NodeLocation[] {
  const found: NodeLocation[] = []
  walk(doc, -1, undefined, undefined, (location) => found.push(location))
  return found
}

function ancestor(
  location: NodeLocation | undefined,
  predicate: (node: GeometryProbeNodeLike) => boolean,
): NodeLocation | undefined {
  for (let current = location; current; current = current.parent)
    if (predicate(current.node)) return current
  return undefined
}

function attrMatches(
  node: GeometryProbeNodeLike,
  expected: Record<string, unknown> | undefined,
): boolean {
  return Object.entries(expected ?? {}).every(([key, value]) => Object.is(node.attrs?.[key], value))
}

function offsetPosition(
  location: NodeLocation,
  offset: GeometryProbeOffset | number = 'middle',
): number {
  if (location.contentEnd < location.contentStart) return location.nodeStart
  if (typeof offset === 'number')
    return Math.max(
      location.contentStart,
      Math.min(location.contentEnd, location.contentStart + offset),
    )
  if (offset === 'start') return location.contentStart
  if (offset === 'end') return location.contentEnd
  return (
    location.contentStart + Math.floor(Math.max(0, location.contentEnd - location.contentStart) / 2)
  )
}

function blockLocation(location: NodeLocation): NodeLocation | undefined {
  return ancestor(location, (node) =>
    ['docParagraph', 'docHeading', 'docListItem', 'docTable', 'docProtected'].includes(
      typeName(node),
    ),
  )
}

function tableContext(location: NodeLocation | undefined): GeometryProbeContext['table'] {
  const cell = ancestor(
    location,
    (node) => typeName(node) === 'docTableCell' || typeName(node) === 'docTableHeader',
  )
  const row = cell?.parent
  const table = row?.parent
  if (
    !cell ||
    !row ||
    !table ||
    typeName(row.node) !== 'docTableRow' ||
    typeName(table.node) !== 'docTable'
  )
    return undefined
  return {
    ...(typeof table.node.attrs?.docxIndex === 'number'
      ? { docxIndex: table.node.attrs.docxIndex }
      : {}),
    row: row.index ?? 0,
    cell: cell.index ?? 0,
  }
}

function contextOf(
  location: NodeLocation,
  extra: Partial<GeometryProbeContext> = {},
): GeometryProbeContext {
  const block = blockLocation(location) ?? location
  return {
    nodeType: typeName(block.node) || undefined,
    ...(typeof block.node.attrs?.docxIndex === 'number'
      ? { docxIndex: block.node.attrs.docxIndex }
      : {}),
    textScript: scriptOf(textContent(block.node)),
    ...(tableContext(location) ? { table: tableContext(location) } : {}),
    ...extra,
  }
}

function matchingNodes(all: readonly NodeLocation[], anchor: NodeProbeAnchor): NodeLocation[] {
  return all.filter(
    (location) =>
      typeName(location.node) === anchor.nodeType &&
      (anchor.docxIndex === undefined || location.node.attrs?.docxIndex === anchor.docxIndex) &&
      attrMatches(location.node, anchor.attrs) &&
      (anchor.docxIndex !== undefined ||
        anchor.nodeType !== 'doc' ||
        location.node.attrs?.docxIndex === anchor.docxIndex),
  )
}

function resolveTable(
  all: readonly NodeLocation[],
  docxIndex: number | undefined,
  occurrence: number | undefined,
): NodeLocation | undefined {
  const tables = all.filter(
    (location) =>
      typeName(location.node) === 'docTable' &&
      (docxIndex === undefined || location.node.attrs?.docxIndex === docxIndex),
  )
  return tables[occurrence ?? 0]
}

function resolveTableChild(
  table: NodeLocation | undefined,
  row: number | 'last',
  cell: number | 'last' | undefined,
): NodeLocation | undefined {
  if (!table) return undefined
  const rows = children(table.node)
    .map((node, index) => ({ node, index }))
    .filter(({ node }) => typeName(node) === 'docTableRow')
  const rowEntry = row === 'last' ? rows.at(-1) : rows[row]
  if (!rowEntry) return undefined
  const rowStart =
    table.contentStart +
    children(table.node)
      .slice(0, rowEntry.index)
      .reduce((sum, child) => sum + nodeSize(child), 0)
  const rowNodeLocation: NodeLocation = {
    node: rowEntry.node,
    nodeStart: rowStart,
    contentStart: rowStart + 1,
    contentEnd: rowStart + nodeSize(rowEntry.node) - 1,
    parent: table,
    index: rowEntry.index,
  }
  if (cell === undefined) return rowNodeLocation
  const cells = children(rowEntry.node).filter(
    (node) => typeName(node) === 'docTableCell' || typeName(node) === 'docTableHeader',
  )
  const cellIndex = cell === 'last' ? cells.length - 1 : cell
  const cellNode = cells[cellIndex]
  if (!cellNode) return undefined
  const cellStart =
    rowNodeLocation.contentStart +
    children(rowEntry.node)
      .slice(0, cellIndex)
      .reduce((sum, child) => sum + nodeSize(child), 0)
  return {
    node: cellNode,
    nodeStart: cellStart,
    contentStart: cellStart + 1,
    contentEnd: cellStart + nodeSize(cellNode) - 1,
    parent: rowNodeLocation,
    index: cellIndex,
  }
}

function candidatePositions(all: readonly NodeLocation[]): number[] {
  const values = new Set<number>()
  for (const location of all) {
    if (typeof location.node.text === 'string') {
      for (let position = location.contentStart; position <= location.contentEnd; position++)
        values.add(position)
    } else if (location.node !== all[0]?.node) {
      values.add(location.contentStart)
      values.add(location.contentEnd)
    }
  }
  return [...values].sort((left, right) => left - right)
}

function resolveMark(
  all: readonly NodeLocation[],
  anchor: MarkRangeProbeAnchor,
): ResolvedProbeAnchor {
  const matches = all.filter((location) => {
    if (typeof location.node.text !== 'string') return false
    const mark = location.node.marks?.find?.((item) => markTypeName(item) === anchor.markType)
    if (!mark) return false
    return (
      anchor.id === undefined ||
      String(mark.attrs?.id ?? '')
        .split(/\s+/u)
        .includes(anchor.id)
    )
  })
  const location = matches[anchor.occurrence ?? 0]
  if (!location) return { reason: 'mark-not-found' }
  const mark = location.node.marks!.find((item) => markTypeName(item) === anchor.markType)!
  return {
    range: { from: location.contentStart, to: location.contentEnd },
    context: contextOf(location, {
      mark: { type: markTypeName(mark), ...(mark.attrs?.id ? { id: String(mark.attrs.id) } : {}) },
    }),
  }
}

function resolveAnchor(
  doc: GeometryProbeDocument,
  probe: GeometryProbe,
  geometry: PresentationGeometry,
): ResolvedProbeAnchor {
  const all = locations(doc)
  const anchor = probe.anchor
  if (anchor.kind === 'header-footer') return { reason: 'presentation-only-no-pm-position' }
  if (anchor.kind === 'page-boundary') {
    const targetPageIndex = anchor.side === 'before-gap' ? anchor.pageIndex - 1 : anchor.pageIndex
    if (targetPageIndex < 0) return { reason: 'page-boundary-not-found' }
    const positions = candidatePositions(all)
      .map((position) => ({ position, geometry: geometry.positionToGeometry(position) }))
      .filter(({ geometry: item }) => item.status === 'resolved' && item.pageIndex !== undefined)
      .filter(({ geometry: item }) => item.pageIndex === targetPageIndex)
    if (positions.length === 0) return { reason: 'page-boundary-not-found' }
    const selected = anchor.side === 'before-gap' ? positions.at(-1) : positions[0]
    return selected ? { position: selected.position } : { reason: 'page-boundary-not-found' }
  }
  if (anchor.kind === 'column' || anchor.kind === 'column-transition') {
    const candidates = candidatePositions(all)
      .map((position) => ({ position, geometry: geometry.positionToGeometry(position) }))
      .filter(({ geometry: item }) => item.status === 'resolved' && item.columnIndex !== undefined)
      .filter(
        ({ geometry: item }) =>
          anchor.pageIndex === undefined || item.pageIndex === anchor.pageIndex,
      )
    if (anchor.kind === 'column') {
      const matches = candidates.filter(
        ({ geometry: item }) => item.columnIndex === anchor.columnIndex,
      )
      const selected = (anchor.side ?? 'first') === 'last' ? matches.at(-1) : matches[0]
      return selected ? { position: selected.position } : { reason: 'column-not-found' }
    }
    const from = candidates
      .filter(({ geometry: item }) => item.columnIndex === anchor.fromColumn)
      .at(-1)
    const to = candidates.filter(({ geometry: item }) => item.columnIndex === anchor.toColumn)[0]
    return from && to
      ? {
          range: {
            from: Math.min(from.position, to.position),
            to: Math.max(from.position, to.position),
          },
        }
      : { reason: 'column-transition-not-found' }
  }
  if (anchor.kind === 'mark-range') return resolveMark(all, anchor)
  if (anchor.kind === 'table-cell' || anchor.kind === 'table-row-boundary') {
    const table = resolveTable(all, anchor.tableDocxIndex, anchor.tableOccurrence)
    const location = resolveTableChild(
      table,
      anchor.row,
      anchor.kind === 'table-cell' ? anchor.cell : undefined,
    )
    if (!location) return { reason: 'anchor-not-found' }
    const context = contextOf(location)
    return {
      position: offsetPosition(location, anchor.offset),
      context,
    }
  }
  if (anchor.kind === 'block') {
    const location = all.find((item) => item.node.attrs?.docxIndex === anchor.docxIndex)
    return location
      ? { position: offsetPosition(location, anchor.offset), context: contextOf(location) }
      : { reason: 'anchor-not-found' }
  }
  if (anchor.kind === 'text') {
    const matches = all.filter(
      (location) =>
        typeof location.node.text === 'string' &&
        location.node.text.includes(anchor.text) &&
        (anchor.nodeType === undefined ||
          (blockLocation(location)?.node &&
            typeName(blockLocation(location)!.node) === anchor.nodeType)) &&
        (anchor.docxIndex === undefined ||
          blockLocation(location)?.node.attrs?.docxIndex === anchor.docxIndex),
    )
    const location = matches[anchor.occurrence ?? 0]
    if (!location) return { reason: 'anchor-not-found' }
    const matchOffset = location.node.text!.indexOf(anchor.text)
    const offset =
      typeof anchor.offset === 'number'
        ? matchOffset + anchor.offset
        : anchor.offset === 'end'
          ? matchOffset + anchor.text.length
          : anchor.offset === 'start'
            ? matchOffset
            : matchOffset + Math.floor(anchor.text.length / 2)
    return { position: offsetPosition(location, offset), context: contextOf(location) }
  }
  const matches = matchingNodes(all, anchor)
  const location = matches[anchor.occurrence ?? 0]
  return location
    ? { position: offsetPosition(location, anchor.offset), context: contextOf(location) }
    : { reason: 'anchor-not-found' }
}

function pointInside(
  rect: NonNullable<PositionGeometry['documentRect']>,
  space: GeometryPoint['space'],
  pageIndex?: number,
): GeometryPoint {
  const x = rect.left + Math.min(0.1, Math.max(0, rect.width / 2))
  const y = rect.top + Math.min(0.1, Math.max(0, rect.height / 2))
  return {
    space,
    x,
    y,
    ...(space === 'page-local' && pageIndex !== undefined ? { pageIndex } : {}),
  }
}

function stablePoints(position: PositionGeometry): GeometryPoint[] {
  const points: GeometryPoint[] = []
  if (position.flowRect) points.push(pointInside(position.flowRect, 'flow'))
  if (position.documentRect) points.push(pointInside(position.documentRect, 'document'))
  if (position.pageLocalRect && position.pageIndex !== undefined)
    points.push(pointInside(position.pageLocalRect, 'page-local', position.pageIndex))
  return points
}

function stablePoint(rect: PositionGeometry['documentRect']): GeometryPoint | undefined {
  if (!rect) return undefined
  return pointInside(rect, 'document')
}

function roundTrip(
  expectedPosition: number,
  actual: PointToPositionResult | undefined,
  position: PositionGeometry,
): GeometryProbeRoundTrip {
  if (!actual || actual.status !== 'resolved' || actual.pmPosition === undefined)
    return {
      status: 'unavailable',
      expectedPosition,
      ...(actual?.reason ? { reason: actual.reason } : {}),
    }
  const delta = actual.pmPosition - expectedPosition
  if (delta === 0)
    return { status: 'exact', expectedPosition, actualPosition: actual.pmPosition, delta }
  if (Math.abs(delta) === 1 && (position.caretRect?.width ?? 0) <= 1.01)
    return {
      status: 'boundary-ambiguous',
      expectedPosition,
      actualPosition: actual.pmPosition,
      delta,
    }
  return { status: 'mismatch', expectedPosition, actualPosition: actual.pmPosition, delta }
}

export function capturePresentationGeometryProbes(
  geometry: PresentationGeometry,
  doc: GeometryProbeDocument | undefined,
  probes: readonly GeometryProbe[],
  pointResolver?: GeometryProbePointResolver,
): GeometryProbeResult[] {
  return probes.map((probe): GeometryProbeResult => {
    if (!doc) {
      return {
        probe,
        status: 'unavailable',
        mappingStatus: 'unavailable',
        reason: 'anchor-not-found',
      }
    }
    const resolved = resolveAnchor(doc, probe, geometry)
    const expectedPosition = resolved.position ?? resolved.range?.from
    if (expectedPosition === undefined) {
      return {
        probe,
        status: 'unavailable',
        mappingStatus: 'unavailable',
        ...(resolved.reason ? { reason: resolved.reason } : {}),
        ...(resolved.range ? { pmRange: resolved.range } : {}),
        ...(resolved.context ? { structuralContext: resolved.context } : {}),
      }
    }
    const positionGeometry = geometry.positionToGeometry(expectedPosition)
    if (positionGeometry.status !== 'resolved') {
      return {
        probe,
        status: 'unavailable',
        mappingStatus: 'unavailable',
        reason: positionGeometry.reason ?? 'editor-coordinates-unavailable',
        ...(resolved.position !== undefined ? { pmPosition: resolved.position } : {}),
        ...(resolved.range ? { pmRange: resolved.range } : {}),
        ...(resolved.context ? { structuralContext: resolved.context } : {}),
        positionGeometry,
      }
    }
    const points = [
      ...(pointResolver?.(expectedPosition, positionGeometry) ?? []),
      ...stablePoints(positionGeometry),
    ]
    let point = points[0] ?? stablePoint(positionGeometry.documentRect)
    let reverse = point ? geometry.pointToPosition(point) : undefined
    for (const candidate of points.slice(1)) {
      const hit = geometry.pointToPosition(candidate)
      if (hit.status === 'resolved') {
        point = candidate
        reverse = hit
        break
      }
    }
    const trip = roundTrip(expectedPosition, reverse, positionGeometry)
    return {
      probe,
      status: 'resolved',
      mappingStatus:
        trip.status === 'exact'
          ? 'resolved'
          : trip.status === 'boundary-ambiguous'
            ? 'boundary-ambiguous'
            : 'unavailable',
      ...(resolved.position !== undefined ? { pmPosition: resolved.position } : {}),
      ...(resolved.range ? { pmRange: resolved.range } : {}),
      ...(positionGeometry.pageIndex !== undefined
        ? {
            pageIndex: positionGeometry.pageIndex,
            pageNumber: pageNumberFromPageIndex(positionGeometry.pageIndex),
          }
        : {}),
      ...(positionGeometry.sectionIndex !== undefined
        ? { sectionIndex: positionGeometry.sectionIndex }
        : {}),
      ...(resolved.context ? { structuralContext: resolved.context } : {}),
      positionGeometry,
      ...(resolved.range
        ? {
            selectionGeometry: geometry.selectionToGeometry(resolved.range.from, resolved.range.to),
          }
        : {}),
      ...(point ? { stablePoint: point } : {}),
      ...(reverse ? { reverse } : {}),
      roundTrip: trip,
    }
  })
}

function failure(
  result: GeometryProbeResult,
  category: DiagnosticCategory,
  path: string,
  expected: unknown,
  actual: unknown,
  delta?: number,
): DiagnosticParityDifference {
  return {
    fixture: result.probe.fixtureId,
    probeId: result.probe.id,
    semanticCase: result.probe.semanticCase,
    category,
    path,
    ...(result.pmPosition !== undefined ? { pmPosition: result.pmPosition } : {}),
    ...(result.pmRange ? { pmRange: result.pmRange } : {}),
    ...(result.pageIndex !== undefined
      ? { pageIndex: result.pageIndex, pageNumber: result.pageNumber }
      : {}),
    ...(result.positionGeometry?.documentRect
      ? { coordinateSpace: result.positionGeometry.documentRect.space }
      : {}),
    mappingStatus: result.mappingStatus,
    expected,
    actual,
    ...(delta !== undefined ? { delta } : {}),
  }
}

export function geometryProbeDiagnostics(
  results: readonly GeometryProbeResult[],
): DiagnosticParityDifference[] {
  const differences: DiagnosticParityDifference[] = []
  for (const result of results) {
    const expected = result.probe.expected
    if (result.status === 'unavailable') {
      if (!result.probe.optional)
        differences.push(
          failure(result, 'mapping', 'probe.status', 'resolved', result.reason ?? 'unavailable'),
        )
      continue
    }
    if (expected?.pageIndex !== undefined && result.pageIndex !== expected.pageIndex)
      differences.push(
        failure(
          result,
          'geometry-page',
          'probe.pageIndex',
          expected.pageIndex,
          result.pageIndex,
          (result.pageIndex ?? 0) - expected.pageIndex,
        ),
      )
    if (expected?.sectionIndex !== undefined && result.sectionIndex !== expected.sectionIndex)
      differences.push(
        failure(
          result,
          'geometry-page',
          'probe.sectionIndex',
          expected.sectionIndex,
          result.sectionIndex,
          (result.sectionIndex ?? 0) - expected.sectionIndex,
        ),
      )
    if (
      expected?.columnIndex !== undefined &&
      result.positionGeometry?.columnIndex !== expected.columnIndex
    )
      differences.push(
        failure(
          result,
          'column',
          'probe.columnIndex',
          expected.columnIndex,
          result.positionGeometry?.columnIndex,
        ),
      )
    if (
      expected?.nodeType !== undefined &&
      result.structuralContext?.nodeType !== expected.nodeType
    )
      differences.push(
        failure(
          result,
          'geometry-position',
          'probe.nodeType',
          expected.nodeType,
          result.structuralContext?.nodeType,
        ),
      )
    if (
      expected?.docxIndex !== undefined &&
      result.structuralContext?.docxIndex !== expected.docxIndex
    )
      differences.push(
        failure(
          result,
          'geometry-position',
          'probe.docxIndex',
          expected.docxIndex,
          result.structuralContext?.docxIndex,
        ),
      )
    if (
      expected?.textScript !== undefined &&
      result.structuralContext?.textScript !== expected.textScript
    )
      differences.push(
        failure(
          result,
          'geometry-position',
          'probe.textScript',
          expected.textScript,
          result.structuralContext?.textScript,
        ),
      )
    if (
      expected?.markType !== undefined &&
      result.structuralContext?.mark?.type !== expected.markType
    )
      differences.push(
        failure(
          result,
          'mapping',
          'probe.markType',
          expected.markType,
          result.structuralContext?.mark?.type,
        ),
      )
    if (expected?.table) {
      const actual = result.structuralContext?.table
      if (actual?.row !== expected.table.row || actual.cell !== expected.table.cell)
        differences.push(failure(result, 'table', 'probe.table', expected.table, actual))
    }
    if (
      !result.probe.optional &&
      (result.roundTrip?.status === 'mismatch' || result.roundTrip?.status === 'unavailable')
    )
      differences.push(
        failure(
          result,
          'geometry-hit-test',
          'probe.roundTrip',
          'exact-or-boundary-ambiguous',
          result.roundTrip,
        ),
      )
  }
  return differences
}
