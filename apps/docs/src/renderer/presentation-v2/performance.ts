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

export interface PresentationV2MeasurementWindowSnapshot {
  totalCandidates: number
  skippedPrefixCandidates: number
  visitedCandidates: number
  restartBlockIndex?: number
  restartPageIndex?: number
  optimized: boolean
}

export interface PresentationV2PerformanceSink {
  onTotal?: (durationMs: number) => void
  onPhase?: (phase: PresentationV2Phase, durationMs: number) => void
  onMeasurementCandidates?: (count: number) => void
  onMeasurementSample?: (kind: PresentationV2MeasurementKind, cacheHit: boolean) => void
  onMeasurementCacheRestore?: (kind: PresentationV2MeasurementKind) => void
  onMeasurementWindow?: (window: PresentationV2MeasurementWindowSnapshot) => void
  onFullRefinementFallback?: (reason: string) => void
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
  measurementCandidatesTotal: number
  measurementCandidatesVisited: number
  measurementCandidatesSkipped: number
  measurementRestartBlockIndex?: number
  measurementRestartPageIndex?: number
  measurementCacheRestores: number
  fullRefinementFallbacks: number
  fullRefinementFallbackReasons: string[]
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
  let measurementCandidatesTotal = 0
  let measurementCandidatesVisited = 0
  let measurementCandidatesSkipped = 0
  let measurementRestartBlockIndex: number | undefined
  let measurementRestartPageIndex: number | undefined
  let measurementCacheRestores = 0
  let fullRefinementFallbacks = 0
  const fullRefinementFallbackReasons: string[] = []

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
    onMeasurementCacheRestore: () => {
      measurementCacheRestores++
    },
    onMeasurementWindow: (window) => {
      measurementCandidatesTotal = Math.max(measurementCandidatesTotal, window.totalCandidates)
      measurementCandidatesVisited += window.visitedCandidates
      measurementCandidatesSkipped += window.skippedPrefixCandidates
      if (window.optimized && measurementRestartBlockIndex === undefined) {
        measurementRestartBlockIndex = window.restartBlockIndex
        measurementRestartPageIndex = window.restartPageIndex
      }
    },
    onFullRefinementFallback: (reason) => {
      fullRefinementFallbacks++
      if (!fullRefinementFallbackReasons.includes(reason))
        fullRefinementFallbackReasons.push(reason)
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
      // The first-pass candidate population can grow after a re-solve exposes
      // new page/column candidates. Include the sampler attempts and cache-only
      // prefix restores so this is a complete run-work count, not just the
      // first candidate discovery result.
      measurementCandidatesTotal: Math.max(
        measurementCandidatesTotal,
        measurementCandidates,
        measurementCandidatesVisited + measurementCandidatesSkipped,
      ),
      measurementCandidatesVisited,
      measurementCandidatesSkipped,
      measurementRestartBlockIndex,
      measurementRestartPageIndex,
      measurementCacheRestores,
      fullRefinementFallbacks,
      fullRefinementFallbackReasons: [...fullRefinementFallbackReasons],
    }),
  }
}
