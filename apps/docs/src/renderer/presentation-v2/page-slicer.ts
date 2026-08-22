import type { PresentationInput } from './index'
import {
  applyBlockMeta,
  computeSectionedSlicesF2,
  insertParityBlanks,
  type PageSlice,
} from '../pagination'
import {
  createPresentationRefinementWindowV2,
  refinePresentationMeasurementsV2,
  type PresentationV2RefinementWindow,
} from './measurement'
import {
  createPresentationMeasurementContextV2,
  shouldInvalidateMeasurementV2,
  type PresentationMeasurementContextV2,
} from './measurement-context'
import type { PresentationInvalidationHint } from './measurement-invalidation'
import type { PresentationV2PerformanceSink } from './performance'
import { normalizePresentationSectionsV2, type PresentationSectionInputsV2 } from './sections'

export type PresentationV2PaginationInput = Pick<
  PresentationInput,
  'blocks' | 'sectionGeoms' | 'totalHeight' | 'zoomFactor' | 'metaOf'
> & {
  invalidationHint?: PresentationInvalidationHint
  /** Dev/test-only correctness oracle; not a user-facing renderer setting. */
  forceFullRefinement?: boolean
  performance?: PresentationV2PerformanceSink
}

const MAX_LINE_REFINEMENT_PASSES = 3

function prepareSemanticPagination(input: PresentationV2PaginationInput): void {
  if (input.metaOf) applyBlockMeta(input.blocks, input.metaOf)
}

function solveInitialPageFlow(
  input: PresentationV2PaginationInput,
  sections: PresentationSectionInputsV2,
): PageSlice[] {
  // Preserve the GenOffice zero-capacity fallback while making the decision
  // explicit in the V2 section phase.
  const geoms = sections.hasUsableGeometry ? sections.geoms : []
  return computeSectionedSlicesF2(input.blocks, geoms, input.totalHeight)
}

function flowPrefixChanged(previous: PageSlice[], next: PageSlice[], restartPageIndex: number) {
  if (
    JSON.stringify(previous.slice(0, restartPageIndex)) !==
    JSON.stringify(next.slice(0, restartPageIndex))
  )
    return true
  const previousBoundary = previous[restartPageIndex]
  const nextBoundary = next[restartPageIndex]
  if (!previousBoundary || !nextBoundary) return true
  return (
    previousBoundary.start !== nextBoundary.start ||
    previousBoundary.section !== nextBoundary.section ||
    JSON.stringify(
      previousBoundary.regions?.map((region) => region.columns.map((column) => column.start)),
    ) !==
      JSON.stringify(
        nextBoundary.regions?.map((region) => region.columns.map((column) => column.start)),
      )
  )
}

function refineMeasuredPageFlow(
  input: PresentationV2PaginationInput,
  initialSlices: PageSlice[],
  sections: PresentationSectionInputsV2,
  initialMeasurementContext: PresentationMeasurementContextV2,
  performance?: PresentationV2PerformanceSink,
): PageSlice[] {
  let slices = initialSlices
  let measurementContext = initialMeasurementContext
  const environmentStable =
    input.invalidationHint?.kind !== 'local' ||
    input.invalidationHint.fontEpoch === measurementContext.fontEpoch
  let refinementWindow: PresentationV2RefinementWindow | undefined =
    input.forceFullRefinement || !environmentStable
      ? undefined
      : createPresentationRefinementWindowV2(input.blocks, slices, input.invalidationHint)
  if (!input.forceFullRefinement) {
    if (input.invalidationHint?.kind === 'local' && !environmentStable)
      performance?.onFullRefinementFallback?.('font-epoch')
    else if (input.invalidationHint?.kind === 'local' && !refinementWindow)
      performance?.onFullRefinementFallback?.('unknown-restart-boundary')
    else if (!input.invalidationHint)
      performance?.onFullRefinementFallback?.('no-invalidation-hint')
  }
  const refinementStartedAt = performance ? globalThis.performance.now() : 0
  // Keep the existing bounded fixed-point behavior: line/table measurement can
  // expose a new page candidate, which requires one more GenOffice re-slice.
  for (let pass = 0; pass < MAX_LINE_REFINEMENT_PASSES; pass++) {
    if (shouldInvalidateMeasurementV2(measurementContext, input.zoomFactor)) {
      measurementContext = createPresentationMeasurementContextV2(input.zoomFactor)
    }
    const changed = refinePresentationMeasurementsV2({
      blocks: input.blocks,
      sectionGeoms: sections.geoms,
      pages: slices,
      zoomFactor: input.zoomFactor,
      metaOf: input.metaOf,
      measurementContext,
      refinementWindow,
      performance,
    })
    performance?.onRefinementPass?.(changed)
    if (!changed) break
    performance?.onResolve?.()
    const previousSlices = slices
    slices = solveInitialPageFlow(input, sections)
    // The first solve intentionally starts from unmeasured BlockBoxes. Its
    // expected re-solve can change the suffix while the restored prefix becomes
    // line-aware. On later passes, compare only the prefix/boundary that the
    // optimization relies on; a change there expands to full V2 refinement.
    if (
      refinementWindow &&
      pass > 0 &&
      refinementWindow.restartPageIndex !== undefined &&
      flowPrefixChanged(previousSlices, slices, refinementWindow.restartPageIndex)
    ) {
      performance?.onFullRefinementFallback?.('page-flow-changed')
      refinementWindow = undefined
      // The restored prefix was valid for the previous boundary. Once that
      // boundary itself moves, clear the transient BlockBox measurements so
      // the next bounded pass really is full V2 refinement rather than a
      // candidate list accidentally filtered by stale fields.
      for (const block of input.blocks) {
        block.lineBoxes = undefined
        block.tableRows = undefined
      }
    }
  }
  if (performance) {
    performance.onPhase?.(
      'measurementRefinement',
      globalThis.performance.now() - refinementStartedAt,
    )
  }
  return slices
}

function finalizePhysicalParity(
  slices: PageSlice[],
  sections: PresentationSectionInputsV2,
): PageSlice[] {
  return insertParityBlanks(slices, sections.geoms)
}

/**
 * V2-owned orchestration over the existing GenOffice pagination primitives.
 * This is intentionally not a second paginator: the shared primitives retain
 * all page, line, table, section, column, and parity decisions.
 */
export function paginatePresentationV2(input: PresentationV2PaginationInput): PageSlice[] {
  const startedAt = input.performance ? globalThis.performance.now() : 0
  prepareSemanticPagination(input)
  const measurementContext = createPresentationMeasurementContextV2(input.zoomFactor)
  const sectionStartedAt = input.performance ? globalThis.performance.now() : 0
  const sections = normalizePresentationSectionsV2(input.sectionGeoms)
  if (input.performance) {
    input.performance.onPhase?.(
      'sectionNormalization',
      globalThis.performance.now() - sectionStartedAt,
    )
  }
  const solveStartedAt = input.performance ? globalThis.performance.now() : 0
  const initialSlices = solveInitialPageFlow(input, sections)
  if (input.performance) {
    input.performance.onPhase?.('initialPageSolve', globalThis.performance.now() - solveStartedAt)
  }
  const refinedSlices = refineMeasuredPageFlow(
    input,
    initialSlices,
    sections,
    measurementContext,
    input.performance,
  )
  const parityStartedAt = input.performance ? globalThis.performance.now() : 0
  const result = finalizePhysicalParity(refinedSlices, sections)
  if (input.performance) {
    input.performance.onPhase?.(
      'parityFinalization',
      globalThis.performance.now() - parityStartedAt,
    )
    input.performance.onTotal?.(globalThis.performance.now() - startedAt)
  }
  return result
}
