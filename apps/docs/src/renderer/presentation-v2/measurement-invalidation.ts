import type { Transaction } from '@tiptap/pm/state'

export type PresentationInvalidationReason =
  | 'local-text'
  | 'unknown-transaction'
  | 'structural-transaction'
  | 'non-history-transaction'
  | 'font-epoch'
  | 'layout-environment'
  | 'prefix-cache-miss'
  | 'page-flow-changed'
  | 'no-invalidation-hint'

export type PresentationScheduleClass = 'FAST_LOCAL' | 'CONSERVATIVE'

/** Benchmark sweep (50/100/150 ms) selected 50 ms: isolated wait fell materially and an 8-edit burst still ran only twice. */
export const PRESENTATION_FAST_LOCAL_DELAY_MS = 50
export const PRESENTATION_FAST_LOCAL_MAX_WAIT_MS = 250
export const PRESENTATION_CONSERVATIVE_DELAY_MS = 300

/**
 * Resolve the narrow local-edit timing policy. The optional global override is
 * a benchmark/debug hook only; production keeps the evidence-selected default.
 */
export function presentationScheduleDelayMs(
  scheduleClass: PresentationScheduleClass,
  now: number,
  firstPendingAt?: number,
): number {
  if (scheduleClass === 'CONSERVATIVE') return PRESENTATION_CONSERVATIVE_DELAY_MS
  const override = (
    globalThis as typeof globalThis & {
      __9profsDocsPresentationLocalDelayMs?: unknown
    }
  ).__9profsDocsPresentationLocalDelayMs
  const localDelay =
    typeof override === 'number' && Number.isFinite(override)
      ? Math.max(0, Math.min(PRESENTATION_CONSERVATIVE_DELAY_MS, override))
      : PRESENTATION_FAST_LOCAL_DELAY_MS
  if (localDelay >= PRESENTATION_CONSERVATIVE_DELAY_MS) return localDelay
  if (firstPendingAt === undefined) return localDelay
  const elapsed = Math.max(0, now - firstPendingAt)
  return Math.min(localDelay, Math.max(0, PRESENTATION_FAST_LOCAL_MAX_WAIT_MS - elapsed))
}

/**
 * Transient presentation-only information about the next layout run.
 *
 * topLevelIndex is the ProseMirror document order, not a persisted block id.
 * blockIndex is filled after the live DOM has been measured and is therefore
 * also ephemeral. Neither value enters PM, DOCX, dirty, or save state.
 */
export interface PresentationInvalidationHint {
  readonly kind: 'local' | 'full'
  readonly topLevelIndex?: number
  readonly blockIndex?: number
  readonly layoutEpoch: number
  readonly fontEpoch: number
  readonly reason: PresentationInvalidationReason
}

function fullHint(
  layoutEpoch: number,
  fontEpoch: number,
  reason: PresentationInvalidationReason,
): PresentationInvalidationHint {
  return Object.freeze({ kind: 'full', layoutEpoch, fontEpoch, reason })
}

export function createFullPresentationInvalidationHint(
  layoutEpoch: number,
  fontEpoch: number,
  reason: PresentationInvalidationReason,
): PresentationInvalidationHint {
  return fullHint(layoutEpoch, fontEpoch, reason)
}

type ChangedRange = {
  oldFrom: number
  oldTo: number
  newFrom: number
  newTo: number
}

function stepType(step: unknown): string | undefined {
  try {
    const json = (step as { toJSON?: () => unknown }).toJSON?.()
    return json && typeof json === 'object' && 'stepType' in json
      ? String((json as { stepType?: unknown }).stepType)
      : undefined
  } catch {
    return undefined
  }
}

function changedRanges(transaction: Transaction): ChangedRange[] {
  const maps = (transaction.mapping as unknown as { maps?: unknown[] }).maps ?? []
  const ranges: ChangedRange[] = []
  for (const map of maps) {
    const forEach = (
      map as {
        forEach?: (
          callback: (oldFrom: number, oldTo: number, newFrom: number, newTo: number) => void,
        ) => void
      }
    ).forEach
    if (typeof forEach !== 'function') return []
    forEach.call(map, (oldFrom, oldTo, newFrom, newTo) => {
      ranges.push({ oldFrom, oldTo, newFrom, newTo })
    })
  }
  return ranges
}

function resolvedAt(doc: Transaction['doc'], position: number) {
  const safe = Math.max(0, Math.min(position, doc.content.size))
  return doc.resolve(safe)
}

function isSingleTextblockRange(doc: Transaction['doc'], from: number, to: number): boolean {
  const start = resolvedAt(doc, from)
  const end = resolvedAt(doc, to)
  return start.parent.isTextblock && end.parent.isTextblock && start.sameParent(end)
}

function topLevelIndex(doc: Transaction['doc'], position: number): number {
  return resolvedAt(doc, position).index(0)
}

/**
 * Convert one PM transaction into the narrow optimization signal. Only a
 * single replace inside one textblock is optimized. Every structural, mapped,
 * or otherwise ambiguous transaction gets an explicit V2 full-refinement hint.
 */
export function presentationInvalidationHintFromTransaction(
  transaction: Transaction,
  layoutEpoch: number,
  fontEpoch: number,
): PresentationInvalidationHint | undefined {
  if (!transaction.docChanged) return undefined
  if (transaction.getMeta('addToHistory') === false)
    return fullHint(layoutEpoch, fontEpoch, 'non-history-transaction')

  const steps = transaction.steps.map(stepType)
  if (steps.some((type) => type !== 'replace'))
    return fullHint(layoutEpoch, fontEpoch, 'structural-transaction')

  const ranges = changedRanges(transaction)
  if (
    transaction.steps.length !== 1 ||
    ranges.length !== 1 ||
    !isSingleTextblockRange(transaction.before, ranges[0].oldFrom, ranges[0].oldTo) ||
    !isSingleTextblockRange(transaction.doc, ranges[0].newFrom, ranges[0].newTo)
  )
    return fullHint(layoutEpoch, fontEpoch, 'unknown-transaction')

  const oldIndex = topLevelIndex(transaction.before, ranges[0].oldFrom)
  const newIndex = topLevelIndex(transaction.doc, ranges[0].newFrom)
  if (oldIndex !== newIndex) return fullHint(layoutEpoch, fontEpoch, 'unknown-transaction')

  return Object.freeze({
    kind: 'local',
    topLevelIndex: newIndex,
    layoutEpoch,
    fontEpoch,
    reason: 'local-text',
  })
}

/** Merge all edits observed before one debounced presentation run. */
export function mergePresentationInvalidationHints(
  current: PresentationInvalidationHint | null | undefined,
  next: PresentationInvalidationHint | undefined,
): PresentationInvalidationHint | null {
  if (!next) return current ?? null
  if (!current) return next
  if (current.kind === 'full' || next.kind === 'full')
    return fullHint(
      next.layoutEpoch,
      next.fontEpoch,
      next.kind === 'full' ? next.reason : current.reason,
    )
  if (
    current.layoutEpoch !== next.layoutEpoch ||
    current.fontEpoch !== next.fontEpoch ||
    current.topLevelIndex === undefined ||
    next.topLevelIndex === undefined
  )
    return fullHint(next.layoutEpoch, next.fontEpoch, 'layout-environment')
  return Object.freeze({
    ...next,
    topLevelIndex: Math.min(current.topLevelIndex, next.topLevelIndex),
  })
}
