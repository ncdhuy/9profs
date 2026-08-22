import type { PresentationInput } from './index'
import {
  paginationMeasurementCandidates,
  restoreCachedPaginationBlock,
  samplePaginationBlock,
  type BlockBox,
  type PageSlice,
} from '../pagination'
import {
  createPresentationMeasurementContextV2,
  shouldInvalidateMeasurementV2,
  type PresentationMeasurementContextV2,
} from './measurement-context'
import type { PresentationV2PerformanceSink } from './performance'
import type { PresentationInvalidationHint } from './measurement-invalidation'

export interface PresentationV2RefinementWindow {
  readonly fromBlockIndex: number
  readonly restartPageIndex?: number
}

function intersectsFlow(blockTop: number, blockHeight: number, start: number, end: number) {
  const blockEnd = blockTop + Math.max(blockHeight, 0.01)
  return blockTop < end && blockEnd > start
}

function flowBoundaryForBlock(block: { top: number; height: number }, pages: PageSlice[]) {
  for (let pageIndex = 0; pageIndex < pages.length; pageIndex++) {
    const page = pages[pageIndex]
    for (const region of page.regions ?? []) {
      for (const column of region.columns) {
        if (intersectsFlow(block.top, block.height, column.start, column.end))
          return { start: column.start, pageIndex }
      }
    }
    if (intersectsFlow(block.top, block.height, page.start, page.end))
      return { start: page.start, pageIndex }
  }
  return pages.length > 0
    ? { start: pages[pages.length - 1].start, pageIndex: pages.length - 1 }
    : null
}

/**
 * Choose a page/column restart boundary before the edited block. The preceding
 * keepNext chain is included so the boundary cannot cut through a placement
 * dependency. Tables remain atomic BlockBoxes and are therefore included as a
 * whole whenever they intersect the chosen flow boundary.
 */
export function createPresentationRefinementWindowV2(
  blocks: BlockBox[],
  pages: PageSlice[],
  hint: PresentationInvalidationHint | undefined,
): PresentationV2RefinementWindow | undefined {
  if (hint?.kind !== 'local' || hint.blockIndex === undefined || blocks.length === 0)
    return undefined
  const target = Math.max(0, Math.min(hint.blockIndex, blocks.length - 1))
  const boundary = flowBoundaryForBlock(blocks[target], pages)
  if (!boundary) return undefined
  let fromBlockIndex = blocks.findIndex((block) =>
    intersectsFlow(block.top, block.height, boundary.start, Number.POSITIVE_INFINITY),
  )
  if (fromBlockIndex < 0) fromBlockIndex = target
  while (fromBlockIndex > 0 && blocks[fromBlockIndex - 1].keepNext) fromBlockIndex--
  return { fromBlockIndex, restartPageIndex: boundary.pageIndex }
}

export type PresentationV2MeasurementInput = Pick<
  PresentationInput,
  'blocks' | 'sectionGeoms' | 'zoomFactor' | 'metaOf'
> & {
  pages: PageSlice[]
  measurementContext?: PresentationMeasurementContextV2
  refinementWindow?: PresentationV2RefinementWindow
  performance?: PresentationV2PerformanceSink
}

/**
 * Own V2 measurement refinement while reusing GenOffice's candidate policy and
 * browser/cache sampler. The sampler mutates the existing BlockBox fields, so
 * the next V2 page-flow solve sees the same inputs as V1.
 */
export function refinePresentationMeasurementsV2(input: PresentationV2MeasurementInput): boolean {
  const measurementContext =
    input.measurementContext &&
    !shouldInvalidateMeasurementV2(input.measurementContext, input.zoomFactor)
      ? input.measurementContext
      : createPresentationMeasurementContextV2(input.zoomFactor)
  let changed = false
  const allCandidates = paginationMeasurementCandidates(
    input.blocks,
    input.sectionGeoms,
    input.pages,
  )
  let candidates = allCandidates
  let skippedPrefixCandidates = 0
  let optimized = false
  if (input.refinementWindow) {
    const { fromBlockIndex } = input.refinementWindow
    const prefixCandidates = allCandidates.filter(
      (candidate) => candidate.blockIndex < fromBlockIndex,
    )
    let prefixReusable = true
    for (const { block, contentHeight } of prefixCandidates) {
      if (
        !restoreCachedPaginationBlock(
          block,
          contentHeight,
          measurementContext.zoomFactor,
          input.metaOf,
          input.performance
            ? {
                onCacheRestore: (kind) => input.performance?.onMeasurementCacheRestore?.(kind),
              }
            : undefined,
        )
      ) {
        prefixReusable = false
        break
      }
    }
    if (prefixReusable) {
      candidates = paginationMeasurementCandidates(input.blocks, input.sectionGeoms, input.pages, {
        fromBlockIndex,
      })
      skippedPrefixCandidates = prefixCandidates.length
      optimized = true
    } else {
      input.performance?.onFullRefinementFallback?.('prefix-cache-miss')
    }
  }
  input.performance?.onMeasurementWindow?.({
    totalCandidates: allCandidates.length,
    skippedPrefixCandidates,
    visitedCandidates: candidates.length,
    restartBlockIndex: input.refinementWindow?.fromBlockIndex,
    restartPageIndex: input.refinementWindow?.restartPageIndex,
    optimized,
  })
  input.performance?.onMeasurementCandidates?.(candidates.length)
  for (const { block, contentHeight } of candidates) {
    if (
      samplePaginationBlock(
        block,
        contentHeight,
        measurementContext.zoomFactor,
        input.metaOf,
        input.performance
          ? {
              onSample: (kind, cacheHit) =>
                input.performance?.onMeasurementSample?.(kind, cacheHit),
            }
          : undefined,
      )
    )
      changed = true
  }
  return changed
}
