/**
 * Opt-in, read-only counters for the V2 pagination seam.
 *
 * This is deliberately a sink rather than a cache or a layout result. The
 * normal V2 path does not allocate or time anything unless a caller supplies
 * the sink (the Electron benchmark does so through the existing page debug
 * seam).
 */
import type { PresentationScheduleClass } from './measurement-invalidation'

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

export interface PresentationSchedulerSnapshot {
  transactionsReceived: number
  fastLocalTransactions: number
  conservativeTransactions: number
  scheduledLayouts: number
  cancelledTimers: number
  rescheduledTimers: number
  mergedInvalidationHints: number
  staleTimerCallbacks: number
  layoutRuns: number
  schedulerWaitMs: number
  layoutExecutionMs: number
  settleMs: number
  transactionToSchedulerAcceptedMs: number
  lastScheduleClass?: PresentationScheduleClass
  lastSchedulerWaitMs?: number
  lastLayoutExecutionMs?: number
  lastSettleMs?: number
  lastTransactionToSchedulerAcceptedMs?: number
}

export interface PresentationSchedulerRecorder {
  onTransaction: (
    scheduleClass: PresentationScheduleClass | undefined,
    merged: boolean,
    timestamp: number,
  ) => void
  onSchedulerAccepted: (scheduleClass: PresentationScheduleClass, timestamp: number) => void
  onTimerScheduled: (rescheduled: boolean) => void
  onTimerCancelled: (rescheduled: boolean) => void
  onStaleTimerCallback: () => void
  onLayoutStart: (scheduled: boolean, timestamp: number) => number
  onLayoutEnd: (runToken: number, timestamp: number) => void
  onSettled: (runToken: number, timestamp: number) => void
  snapshot: () => PresentationSchedulerSnapshot
}

/** Create opt-in scheduler counters for one Docs presentation lifecycle. */
export function createPresentationSchedulerRecorder(): PresentationSchedulerRecorder {
  let transactionsReceived = 0
  let fastLocalTransactions = 0
  let conservativeTransactions = 0
  let scheduledLayouts = 0
  let cancelledTimers = 0
  let rescheduledTimers = 0
  let mergedInvalidationHints = 0
  let staleTimerCallbacks = 0
  let layoutRuns = 0
  let schedulerWaitMs = 0
  let layoutExecutionMs = 0
  let settleMs = 0
  let transactionToSchedulerAcceptedMs = 0
  let lastScheduleClass: PresentationScheduleClass | undefined
  let lastSchedulerWaitMs: number | undefined
  let lastLayoutExecutionMs: number | undefined
  let lastSettleMs: number | undefined
  let lastTransactionToSchedulerAcceptedMs: number | undefined
  let pendingTransactionAt: number | undefined
  let pendingScheduleAt: number | undefined
  let nextRunToken = 0
  let activeRunToken = 0
  let activeRunStartedAt: number | undefined
  let activeRunEndedAt: number | undefined

  return {
    onTransaction: (scheduleClass, merged, timestamp) => {
      transactionsReceived++
      if (scheduleClass === 'FAST_LOCAL') fastLocalTransactions++
      else if (scheduleClass === 'CONSERVATIVE') conservativeTransactions++
      if (merged) mergedInvalidationHints++
      if (scheduleClass !== undefined && pendingTransactionAt === undefined)
        pendingTransactionAt = timestamp
    },
    onSchedulerAccepted: (scheduleClass, timestamp) => {
      lastScheduleClass = scheduleClass
      if (pendingTransactionAt !== undefined) {
        lastTransactionToSchedulerAcceptedMs = timestamp - pendingTransactionAt
        transactionToSchedulerAcceptedMs += lastTransactionToSchedulerAcceptedMs
        pendingTransactionAt = undefined
      }
      pendingScheduleAt = timestamp
    },
    onTimerScheduled: (rescheduled) => {
      scheduledLayouts++
      if (rescheduled) rescheduledTimers++
    },
    onTimerCancelled: (_rescheduled) => {
      cancelledTimers++
    },
    onStaleTimerCallback: () => {
      staleTimerCallbacks++
    },
    onLayoutStart: (scheduled, timestamp) => {
      const runToken = ++nextRunToken
      activeRunToken = runToken
      layoutRuns++
      if (scheduled && pendingScheduleAt !== undefined) {
        lastSchedulerWaitMs = timestamp - pendingScheduleAt
        schedulerWaitMs += lastSchedulerWaitMs
        pendingScheduleAt = undefined
      }
      activeRunStartedAt = timestamp
      activeRunEndedAt = undefined
      return runToken
    },
    onLayoutEnd: (runToken, timestamp) => {
      if (runToken !== activeRunToken || activeRunStartedAt === undefined) return
      lastLayoutExecutionMs = timestamp - activeRunStartedAt
      layoutExecutionMs += lastLayoutExecutionMs
      activeRunEndedAt = timestamp
    },
    onSettled: (runToken, timestamp) => {
      if (runToken !== activeRunToken) return
      if (activeRunEndedAt === undefined) return
      lastSettleMs = timestamp - activeRunEndedAt
      settleMs += lastSettleMs
    },
    snapshot: () => ({
      transactionsReceived,
      fastLocalTransactions,
      conservativeTransactions,
      scheduledLayouts,
      cancelledTimers,
      rescheduledTimers,
      mergedInvalidationHints,
      staleTimerCallbacks,
      layoutRuns,
      schedulerWaitMs,
      layoutExecutionMs,
      settleMs,
      transactionToSchedulerAcceptedMs,
      lastScheduleClass,
      lastSchedulerWaitMs,
      lastLayoutExecutionMs,
      lastSettleMs,
      lastTransactionToSchedulerAcceptedMs,
    }),
  }
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
