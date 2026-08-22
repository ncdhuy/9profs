import type { SectionGeom } from '../pagination'

export type PresentationSectionTransitionV2 = 'initial' | 'page' | 'column' | 'continuous'

/**
 * V2's normalized execution view over the existing pagination-facing geometry.
 * It deliberately contains no document section configuration or persistence state.
 */
export interface PresentationSectionInputsV2 {
  /** Canonical copies of the existing SectionGeom values; numeric values are preserved. */
  readonly geoms: SectionGeom[]
  /** Existing break markers interpreted once for this pagination run. */
  readonly transitions: readonly PresentationSectionTransitionV2[]
  /** Mirrors computeSectionedSlicesF2's empty/zero-capacity fallback condition. */
  readonly hasUsableGeometry: boolean
}

/**
 * Normalize derived SectionGeom inputs for one V2 pagination run.
 *
 * The GenOffice paginator already treats absent columns as one column and
 * absent column-break markers as false. Canonicalizing those values here makes
 * that V2 policy explicit without changing content heights, break types, or
 * column widths, and without mutating the caller's geometry objects.
 */
export function normalizePresentationSectionsV2(
  sectionGeoms: readonly SectionGeom[],
): PresentationSectionInputsV2 {
  const geoms = sectionGeoms.map((geom) => ({
    ...geom,
    cols: Math.max(1, geom.cols ?? 1),
    ...(geom.colWidths ? { colWidths: [...geom.colWidths] } : {}),
    colBreakStart: Boolean(geom.colBreakStart),
  }))

  const transitions = geoms.map<PresentationSectionTransitionV2>((geom, index) => {
    if (index === 0) return 'initial'
    if (geom.forceBreak) return 'page'
    if (geom.colBreakStart) return 'column'
    return 'continuous'
  })

  return {
    geoms,
    transitions,
    hasUsableGeometry:
      sectionGeoms.length > 0 && !sectionGeoms.every((geom) => geom.contentHeight <= 0),
  }
}
