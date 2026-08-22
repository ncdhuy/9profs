import type { PresentationInput } from './index'
import {
  paginationMeasurementCandidates,
  samplePaginationBlock,
  type PageSlice,
} from '../pagination'

export type PresentationV2MeasurementInput = Pick<
  PresentationInput,
  'blocks' | 'sectionGeoms' | 'zoomFactor' | 'metaOf'
> & {
  pages: PageSlice[]
}

/**
 * Own V2 measurement refinement while reusing GenOffice's candidate policy and
 * browser/cache sampler. The sampler mutates the existing BlockBox fields, so
 * the next V2 page-flow solve sees the same inputs as V1.
 */
export function refinePresentationMeasurementsV2(input: PresentationV2MeasurementInput): boolean {
  let changed = false
  const candidates = paginationMeasurementCandidates(input.blocks, input.sectionGeoms, input.pages)
  for (const { block, contentHeight } of candidates) {
    if (samplePaginationBlock(block, contentHeight, input.zoomFactor, input.metaOf)) changed = true
  }
  return changed
}
