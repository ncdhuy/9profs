import type { PresentationInput } from './index'
import {
  applyBlockMeta,
  computeSectionedSlicesF2,
  insertParityBlanks,
  type PageSlice,
} from '../pagination'
import { refinePresentationMeasurementsV2 } from './measurement'
import { normalizePresentationSectionsV2, type PresentationSectionInputsV2 } from './sections'

export type PresentationV2PaginationInput = Pick<
  PresentationInput,
  'blocks' | 'sectionGeoms' | 'totalHeight' | 'zoomFactor' | 'metaOf'
>

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

function refineMeasuredPageFlow(
  input: PresentationV2PaginationInput,
  initialSlices: PageSlice[],
  sections: PresentationSectionInputsV2,
): PageSlice[] {
  let slices = initialSlices
  // Keep the existing bounded fixed-point behavior: line/table measurement can
  // expose a new page candidate, which requires one more GenOffice re-slice.
  for (let pass = 0; pass < MAX_LINE_REFINEMENT_PASSES; pass++) {
    const changed = refinePresentationMeasurementsV2({
      blocks: input.blocks,
      sectionGeoms: sections.geoms,
      pages: slices,
      zoomFactor: input.zoomFactor,
      metaOf: input.metaOf,
    })
    if (!changed) break
    slices = solveInitialPageFlow(input, sections)
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
  prepareSemanticPagination(input)
  const sections = normalizePresentationSectionsV2(input.sectionGeoms)
  const initialSlices = solveInitialPageFlow(input, sections)
  const refinedSlices = refineMeasuredPageFlow(input, initialSlices, sections)
  return finalizePhysicalParity(refinedSlices, sections)
}
