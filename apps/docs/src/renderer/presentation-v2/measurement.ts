import type { PresentationInput } from './index'
import {
  paginationMeasurementCandidates,
  samplePaginationBlock,
  type PageSlice,
} from '../pagination'
import {
  createPresentationMeasurementContextV2,
  shouldInvalidateMeasurementV2,
  type PresentationMeasurementContextV2,
} from './measurement-context'
import type { PresentationV2PerformanceSink } from './performance'

export type PresentationV2MeasurementInput = Pick<
  PresentationInput,
  'blocks' | 'sectionGeoms' | 'zoomFactor' | 'metaOf'
> & {
  pages: PageSlice[]
  measurementContext?: PresentationMeasurementContextV2
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
  const candidates = paginationMeasurementCandidates(input.blocks, input.sectionGeoms, input.pages)
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
