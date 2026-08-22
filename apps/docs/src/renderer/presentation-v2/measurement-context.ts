import { getLineSampleFontEpoch } from '../pagination'

/**
 * Immutable environment snapshot for one V2 pagination run.
 *
 * The shared GenOffice sampler remains the cache authority. This context only
 * makes V2's run-level reuse boundary explicit: font epoch and DOM scale.
 * Text, width, and table-geometry signatures stay in the shared sampler.
 */
export interface PresentationMeasurementContextV2 {
  readonly fontEpoch: number
  readonly zoomFactor: number
}

export function createPresentationMeasurementContextV2(
  zoomFactor: number,
): PresentationMeasurementContextV2 {
  return Object.freeze({ fontEpoch: getLineSampleFontEpoch(), zoomFactor })
}

/**
 * A context becomes stale only when the shared font environment or requested
 * DOM-to-layout scale changes. No cache is cleared here; the shared sampler
 * rechecks its compatible identity signature on the next sample.
 */
export function shouldInvalidateMeasurementV2(
  context: PresentationMeasurementContextV2,
  zoomFactor: number,
): boolean {
  return context.fontEpoch !== getLineSampleFontEpoch() || context.zoomFactor !== zoomFactor
}
