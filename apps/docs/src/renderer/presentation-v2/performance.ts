/**
 * Opt-in, read-only counters for the V2 pagination seam.
 *
 * This is deliberately a sink rather than a cache or a layout result. The
 * normal V2 path does not allocate or time anything unless a caller supplies
 * the sink (the Electron benchmark does so through the existing page debug
 * seam).
 */
export type PresentationV2Phase =
  'sectionNormalization' | 'initialPageSolve' | 'measurementRefinement' | 'parityFinalization'

export type PresentationV2MeasurementKind = 'line' | 'table'

export interface PresentationV2PerformanceSink {
  onTotal?: (durationMs: number) => void
  onPhase?: (phase: PresentationV2Phase, durationMs: number) => void
  onMeasurementCandidates?: (count: number) => void
  onMeasurementSample?: (kind: PresentationV2MeasurementKind, cacheHit: boolean) => void
  onRefinementPass?: (changed: boolean) => void
  onResolve?: () => void
}

export interface PresentationV2PerformanceSnapshot {
  totalMs: number
  sectionNormalizationMs: number
  initialPageSolveMs: number
  measurementRefinementMs: number
  parityFinalizationMs: number
  refinementPasses: number
  reSolves: number
  measurementCandidates: number
  measurementAttempts: number
  actualDomSamples: number
  cacheHits: number
  cacheMisses: number
  lineDomSamples: number
  tableDomSamples: number
}

export interface PresentationV2PerformanceRecorder {
  readonly sink: PresentationV2PerformanceSink
  snapshot(): PresentationV2PerformanceSnapshot
}

/** Create one recorder for one coherent V2 pagination run. */
export function createPresentationV2PerformanceRecorder(): PresentationV2PerformanceRecorder {
  const phases: Record<PresentationV2Phase, number> = {
    sectionNormalization: 0,
    initialPageSolve: 0,
    measurementRefinement: 0,
    parityFinalization: 0,
  }
  let totalMs = 0
  let refinementPasses = 0
  let reSolves = 0
  let measurementCandidates = 0
  let measurementAttempts = 0
  let actualDomSamples = 0
  let cacheHits = 0
  let cacheMisses = 0
  let lineDomSamples = 0
  let tableDomSamples = 0

  const sink: PresentationV2PerformanceSink = {
    onTotal: (durationMs) => {
      totalMs = durationMs
    },
    onPhase: (phase, durationMs) => {
      phases[phase] += durationMs
    },
    onMeasurementCandidates: (count) => {
      measurementCandidates += count
    },
    onMeasurementSample: (kind, cacheHit) => {
      measurementAttempts++
      if (cacheHit) {
        cacheHits++
        return
      }
      cacheMisses++
      actualDomSamples++
      if (kind === 'line') lineDomSamples++
      else tableDomSamples++
    },
    onRefinementPass: () => {
      refinementPasses++
    },
    onResolve: () => {
      reSolves++
    },
  }

  return {
    sink,
    snapshot: () => ({
      totalMs,
      sectionNormalizationMs: phases.sectionNormalization,
      initialPageSolveMs: phases.initialPageSolve,
      measurementRefinementMs: phases.measurementRefinement,
      parityFinalizationMs: phases.parityFinalization,
      refinementPasses,
      reSolves,
      measurementCandidates,
      measurementAttempts,
      actualDomSamples,
      cacheHits,
      cacheMisses,
      lineDomSamples,
      tableDomSamples,
    }),
  }
}
