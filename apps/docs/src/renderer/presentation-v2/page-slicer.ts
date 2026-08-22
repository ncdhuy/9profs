import type { PresentationInput } from './index'
import {
  applyBlockMeta,
  computeSectionedSlicesF2,
  insertParityBlanks,
  type PageSlice,
} from '../pagination'
import { refinePresentationMeasurementsV2 } from './measurement'

export type PresentationV2PaginationInput = Pick<
  PresentationInput,
  'blocks' | 'sectionGeoms' | 'totalHeight' | 'zoomFactor' | 'metaOf'
>

const MAX_LINE_REFINEMENT_PASSES = 3

function prepareSemanticPagination(input: PresentationV2PaginationInput): void {
  if (input.metaOf) applyBlockMeta(input.blocks, input.metaOf)
}

function solveInitialPageFlow(input: PresentationV2PaginationInput): PageSlice[] {
  return computeSectionedSlicesF2(input.blocks, input.sectionGeoms, input.totalHeight)
}

function refineMeasuredPageFlow(
  input: PresentationV2PaginationInput,
  initialSlices: PageSlice[],
): PageSlice[] {
  let slices = initialSlices
  // Keep the existing bounded fixed-point behavior: line/table measurement can
  // expose a new page candidate, which requires one more GenOffice re-slice.
  for (let pass = 0; pass < MAX_LINE_REFINEMENT_PASSES; pass++) {
    const changed = refinePresentationMeasurementsV2({
      blocks: input.blocks,
      sectionGeoms: input.sectionGeoms,
      pages: slices,
      zoomFactor: input.zoomFactor,
      metaOf: input.metaOf,
    })
    if (!changed) break
    slices = solveInitialPageFlow(input)
  }
  return slices
}

function finalizePhysicalParity(
  slices: PageSlice[],
  sectionGeoms: PresentationV2PaginationInput['sectionGeoms'],
): PageSlice[] {
  return insertParityBlanks(slices, sectionGeoms)
}

/**
 * V2-owned orchestration over the existing GenOffice pagination primitives.
 * This is intentionally not a second paginator: the shared primitives retain
 * all page, line, table, section, column, and parity decisions.
 */
export function paginatePresentationV2(input: PresentationV2PaginationInput): PageSlice[] {
  prepareSemanticPagination(input)
  const initialSlices = solveInitialPageFlow(input)
  const refinedSlices = refineMeasuredPageFlow(input, initialSlices)
  return finalizePhysicalParity(refinedSlices, input.sectionGeoms)
}
