import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { Editor } from '@tiptap/core'
import { NodeSelection } from '@tiptap/pm/state'
import type {
  CitationReviewItem,
  CitationReviewRun,
  CoreTransport,
  ResearchCase,
  ResearchClaimEvidenceRelation,
  ResearchSource,
} from '@genoffice/9profs-core'
import { buildManuscriptCitationReviewInput } from '../editor/manuscript-citations'
import { findCitationNodePosition } from '../editor/citation-review-navigation'
import { useI18n, type StringKey } from '../i18n/locale'

export interface CitationReviewPanelProps {
  readonly editor: Editor
  readonly documentId: string
  readonly transport: CoreTransport | null
  readonly onClose: () => void
}

type ReviewState =
  | { kind: 'setup' }
  | { kind: 'running' }
  | { kind: 'loaded'; run: CitationReviewRun; items: CitationReviewItem[] }
  | { kind: 'error'; message: string }

type BusyAction = 'case' | 'source' | 'review' | null
type ReviewFilter = 'all' | 'needs' | ResearchClaimEvidenceRelation

const STATUS_KEYS: Record<CitationReviewItem['status'], StringKey> = {
  unresolved_reference: 'citationReviewStatusUnresolvedReference',
  ambiguous_reference: 'citationReviewStatusAmbiguousReference',
  reference_requires_confirmation: 'citationReviewStatusRequiresConfirmation',
  source_matched_not_verification_ready: 'citationReviewStatusSourceNotReady',
  binding_conflict: 'citationReviewStatusBindingConflict',
  ready_for_verification: 'citationReviewStatusReadyForVerification',
  verification_running: 'citationReviewStatusVerificationRunning',
  verification_completed: 'citationReviewStatusVerificationCompleted',
  verification_failed: 'citationReviewStatusVerificationFailed',
  resolution_failed: 'citationReviewStatusResolutionFailed',
}

const RELATION_KEYS: Record<ResearchClaimEvidenceRelation, StringKey> = {
  supports: 'citationReviewRelationSupports',
  contradicts: 'citationReviewRelationContradicts',
  contextualizes: 'citationReviewRelationContextualizes',
  insufficient: 'citationReviewRelationInsufficient',
}

const RUN_STATUS_KEYS: Record<CitationReviewRun['status'], StringKey> = {
  running: 'citationReviewRunRunning',
  completed: 'citationReviewRunCompleted',
  failed: 'citationReviewRunFailed',
}

const ATTENTION_STATUSES = new Set<CitationReviewItem['status']>([
  'unresolved_reference',
  'ambiguous_reference',
  'reference_requires_confirmation',
  'source_matched_not_verification_ready',
  'binding_conflict',
  'ready_for_verification',
  'verification_running',
  'verification_failed',
  'resolution_failed',
])

const CANDIDATE_CONFIRMATION_STATUSES = new Set<CitationReviewItem['status']>([
  'reference_requires_confirmation',
  'ambiguous_reference',
])

export function citationReviewNeedsAttention(item: Pick<CitationReviewItem, 'status'>): boolean {
  return ATTENTION_STATUSES.has(item.status)
}

/** Historical candidates are displayed for audit, but only these effective item states allow action. */
export function citationReviewAllowsCandidateConfirmation(
  item: Pick<CitationReviewItem, 'status' | 'candidates'>,
): boolean {
  return CANDIDATE_CONFIRMATION_STATUSES.has(item.status) && item.candidates.length > 0
}

function textOrDash(value: string | null | undefined): string {
  return value && value.trim() ? value : '—'
}

function errorMessage(error: unknown, fallback: string): string {
  // Transport failures may contain implementation details or stack traces; the
  // panel exposes a stable, translated message and leaves diagnostics in Core.
  return error instanceof Error && error.name === 'AbortError' ? fallback : fallback
}

function itemRelation(item: CitationReviewItem): ResearchClaimEvidenceRelation | null {
  return item.verification?.relation ?? item.evidence[0]?.relation ?? null
}

export function CitationReviewPanel({
  editor,
  documentId,
  transport,
  onClose,
}: CitationReviewPanelProps) {
  const { t } = useI18n()
  const translateRef = useRef(t)
  translateRef.current = t
  const [review, setReview] = useState<ReviewState>({ kind: 'setup' })
  const [cases, setCases] = useState<ResearchCase[]>([])
  const [caseId, setCaseId] = useState('')
  const [sources, setSources] = useState<ResearchSource[]>([])
  const [sourceId, setSourceId] = useState('')
  const [newCaseTitle, setNewCaseTitle] = useState('')
  const [newSourceLabel, setNewSourceLabel] = useState('')
  const [busy, setBusy] = useState<BusyAction>(null)
  const [filter, setFilter] = useState<ReviewFilter>('all')
  const [message, setMessage] = useState<string | null>(null)
  const [currentVersion, setCurrentVersion] = useState<number | null>(null)
  const [confirmedEntries, setConfirmedEntries] = useState<Set<string>>(new Set())
  const [busyCandidateId, setBusyCandidateId] = useState<string | null>(null)
  const [requiresNewReview, setRequiresNewReview] = useState(false)

  useEffect(() => {
    let disposed = false
    setCases([])
    setCaseId('')
    setSources([])
    setSourceId('')
    setMessage(null)
    setReview({ kind: 'setup' })
    setCurrentVersion(null)
    setRequiresNewReview(false)
    setConfirmedEntries(new Set())
    if (!transport) {
      setReview({ kind: 'error', message: translateRef.current('citationReviewCoreUnavailable') })
      return () => {
        disposed = true
      }
    }
    void transport
      .researchCases()
      .then((next) => {
        if (disposed) return
        setCases(next)
        if (next.length === 1) setCaseId(next[0].caseId)
        setReview({ kind: 'setup' })
      })
      .catch((error: unknown) => {
        if (!disposed) {
          setReview({
            kind: 'error',
            message: errorMessage(error, translateRef.current('citationReviewUnavailable')),
          })
        }
      })
    return () => {
      disposed = true
    }
  }, [documentId, transport])

  useEffect(() => {
    let disposed = false
    setSources([])
    setSourceId('')
    if (!transport || !caseId) return
    void transport
      .researchSources(caseId)
      .then((next) => {
        if (disposed) return
        const manuscriptSources = next.filter(
          (source) => source.researchCaseId === caseId && source.kind === 'manuscript',
        )
        setSources(manuscriptSources)
        if (manuscriptSources.length === 1) setSourceId(manuscriptSources[0].sourceId)
      })
      .catch((error: unknown) => {
        if (!disposed) {
          setMessage(errorMessage(error, translateRef.current('citationReviewSourcesUnavailable')))
        }
      })
    return () => {
      disposed = true
    }
  }, [caseId, transport])

  const createCase = async () => {
    const title = newCaseTitle.trim()
    if (!transport || !title || busy) return
    setBusy('case')
    setMessage(null)
    try {
      const created = await transport.createResearchCase({ title })
      setCases((current) => [...current, created])
      setCaseId(created.caseId)
      setNewCaseTitle('')
    } catch (error: unknown) {
      setMessage(errorMessage(error, translateRef.current('citationReviewCreateFailed')))
    } finally {
      setBusy(null)
    }
  }

  const createSource = async () => {
    const label = newSourceLabel.trim()
    if (!transport || !caseId || !label || busy) return
    setBusy('source')
    setMessage(null)
    try {
      const created = await transport.createResearchSource({
        researchCaseId: caseId,
        kind: 'manuscript',
        label,
      })
      setSources((current) => [...current, created])
      setSourceId(created.sourceId)
      setNewSourceLabel('')
    } catch (error: unknown) {
      setMessage(errorMessage(error, translateRef.current('citationReviewCreateFailed')))
    } finally {
      setBusy(null)
    }
  }

  const startReview = useCallback(async () => {
    if (!transport || !caseId || !sourceId || busy) return
    setBusy('review')
    setReview({ kind: 'running' })
    setMessage(null)
    setRequiresNewReview(false)
    try {
      const active = await transport.activeDocument(documentId)
      if (
        active.documentId !== documentId ||
        active.availability !== 'available' ||
        !Number.isInteger(active.version) ||
        active.version < 0
      ) {
        throw new Error('active document unavailable')
      }
      setCurrentVersion(active.version)
      const input = buildManuscriptCitationReviewInput({
        editor,
        activeDocument: active,
        manuscriptSourceId: sourceId,
      })
      if (input.citations.length === 0) {
        setReview({ kind: 'setup' })
        setMessage(translateRef.current('citationReviewNoCitations'))
        return
      }
      const run = await transport.startManuscriptCitationReview(caseId, input)
      const items = await transport.manuscriptCitationReviewItems(run.reviewRunId)
      setConfirmedEntries(new Set())
      setReview({ kind: 'loaded', run, items })
      setFilter('all')
    } catch (error: unknown) {
      setReview({
        kind: 'error',
        message: errorMessage(error, translateRef.current('citationReviewRunError')),
      })
    } finally {
      setBusy(null)
    }
  }, [busy, caseId, documentId, editor, sourceId, transport])

  const reviewRunId = review.kind === 'loaded' ? review.run.reviewRunId : null
  useEffect(() => {
    if (!transport || !reviewRunId) return
    let disposed = false
    const refreshVersion = () => {
      setCurrentVersion(null)
      void transport
        .activeDocument(documentId)
        .then((active) => {
          if (
            !disposed &&
            active.documentId === documentId &&
            active.availability === 'available'
          ) {
            setCurrentVersion(active.version)
          }
        })
        .catch(() => {
          // Unknown freshness is intentionally treated as stale by the render path.
        })
    }
    editor.on('update', refreshVersion)
    return () => {
      disposed = true
      editor.off('update', refreshVersion)
    }
  }, [documentId, editor, reviewRunId, transport])

  const loaded = review.kind === 'loaded' ? review : null
  const stale =
    loaded !== null && (currentVersion === null || currentVersion !== loaded.run.documentVersion)
  const reviewInvalidated = stale || requiresNewReview
  const sourceLabels = useMemo(
    () => new Map(sources.map((source) => [source.sourceId, source.label])),
    [sources],
  )
  const filteredItems = useMemo(() => {
    if (!loaded) return []
    return loaded.items.filter((item) => {
      if (filter === 'needs') return citationReviewNeedsAttention(item)
      if (filter === 'all') return true
      return itemRelation(item) === filter
    })
  }, [filter, loaded])
  const firstVisibleCandidateItemByEntry = useMemo(() => {
    const first = new Map<string, string>()
    for (const item of filteredItems) {
      if (!citationReviewAllowsCandidateConfirmation(item)) continue
      for (const candidate of item.candidates) {
        if (!first.has(candidate.resolutionEntryId)) {
          first.set(candidate.resolutionEntryId, item.itemId)
        }
      }
    }
    return first
  }, [filteredItems])

  const goToCitation = (item: CitationReviewItem) => {
    if (reviewInvalidated) {
      setMessage(
        requiresNewReview
          ? translateRef.current('citationReviewNewReviewRequired')
          : translateRef.current('citationReviewStale'),
      )
      return
    }
    const position = findCitationNodePosition(editor, item)
    if (position === null) {
      setMessage(translateRef.current('citationReviewNavigationFailed'))
      return
    }
    editor.view.dispatch(
      editor.state.tr
        .setSelection(NodeSelection.create(editor.state.doc, position))
        .scrollIntoView(),
    )
    editor.commands.focus()
  }

  const confirmCandidate = async (candidate: CitationReviewItem['candidates'][number]) => {
    if (!transport || !loaded || !loaded.run.referenceResolutionRunId || reviewInvalidated) {
      setMessage(
        requiresNewReview
          ? translateRef.current('citationReviewNewReviewRequired')
          : translateRef.current('citationReviewConfirmationBlocked'),
      )
      return
    }
    if (busyCandidateId) return
    setBusyCandidateId(candidate.candidateId)
    setBusy('review')
    setMessage(null)
    try {
      await transport.confirmManuscriptReferenceCandidate(
        loaded.run.referenceResolutionRunId,
        candidate.resolutionEntryId,
        candidate.candidateId,
      )
      setConfirmedEntries((current) => new Set(current).add(candidate.resolutionEntryId))
      setRequiresNewReview(true)
      setMessage(translateRef.current('citationReviewConfirmedNeedsRecheck'))
    } catch (error: unknown) {
      setMessage(errorMessage(error, translateRef.current('citationReviewConfirmationFailed')))
    } finally {
      setBusyCandidateId(null)
      setBusy(null)
    }
  }

  const caseSelected = cases.some((researchCase) => researchCase.caseId === caseId)
  const sourceSelected = sources.some((source) => source.sourceId === sourceId)
  const startDisabled = !transport || !caseSelected || !sourceSelected || busy !== null
  const total = loaded?.items.length ?? 0
  const relationCounts = loaded
    ? loaded.items.reduce<Record<ResearchClaimEvidenceRelation, number>>(
        (counts, item) => {
          const relation = itemRelation(item)
          if (relation) counts[relation] += 1
          return counts
        },
        { supports: 0, contradicts: 0, contextualizes: 0, insufficient: 0 },
      )
    : null

  return (
    <aside className="comments-pane citation-review-pane" aria-label={t('citationReviewTitle')}>
      <div className="comments-pane-head">
        <div>
          <strong className="comments-pane-title">{t('citationReviewTitle')}</strong>
          <span className="citation-review-mode">{t('citationReviewReadOnly')}</span>
        </div>
        <button
          type="button"
          className="comments-pane-close"
          aria-label={t('citationReviewClose')}
          onClick={onClose}
        >
          ×
        </button>
      </div>

      <div className="citation-review-scroll">
        <section
          className="citation-review-context"
          aria-labelledby="citation-review-context-title"
        >
          <h2 id="citation-review-context-title">{t('citationReviewContext')}</h2>
          <label htmlFor="citation-review-case">{t('citationReviewCase')}</label>
          <select
            id="citation-review-case"
            value={caseId}
            disabled={busy !== null}
            onChange={(event) => {
              setCaseId(event.target.value)
              setSourceId('')
              setReview({ kind: 'setup' })
              setMessage(null)
            }}
          >
            <option value="">{t('citationReviewSelectCase')}</option>
            {cases.map((researchCase) => (
              <option key={researchCase.caseId} value={researchCase.caseId}>
                {researchCase.title}
              </option>
            ))}
          </select>
          <div className="citation-review-create-row">
            <input
              value={newCaseTitle}
              disabled={busy !== null || !transport}
              aria-label={t('citationReviewNewCase')}
              placeholder={t('citationReviewNewCase')}
              onChange={(event) => setNewCaseTitle(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') void createCase()
              }}
            />
            <button
              type="button"
              className="btn-ghost"
              disabled={!newCaseTitle.trim() || busy !== null || !transport}
              onClick={() => void createCase()}
            >
              {t('citationReviewCreateCase')}
            </button>
          </div>

          <label htmlFor="citation-review-source">{t('citationReviewManuscriptSource')}</label>
          <select
            id="citation-review-source"
            value={sourceId}
            disabled={busy !== null || !caseSelected}
            onChange={(event) => {
              setSourceId(event.target.value)
              setReview({ kind: 'setup' })
              setMessage(null)
            }}
          >
            <option value="">{t('citationReviewSelectSource')}</option>
            {sources.map((source) => (
              <option key={source.sourceId} value={source.sourceId}>
                {source.label}
              </option>
            ))}
          </select>
          <div className="citation-review-create-row">
            <input
              value={newSourceLabel}
              disabled={busy !== null || !caseSelected || !transport}
              aria-label={t('citationReviewNewSource')}
              placeholder={t('citationReviewNewSource')}
              onChange={(event) => setNewSourceLabel(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') void createSource()
              }}
            />
            <button
              type="button"
              className="btn-ghost"
              disabled={!newSourceLabel.trim() || busy !== null || !caseSelected || !transport}
              onClick={() => void createSource()}
            >
              {t('citationReviewCreateSource')}
            </button>
          </div>

          <button
            type="button"
            className="btn-primary citation-review-start"
            disabled={startDisabled}
            onClick={() => void startReview()}
          >
            {busy === 'review'
              ? t('citationReviewLoading')
              : loaded
                ? t('citationReviewRecheck')
                : t('citationReviewStart')}
          </button>
          {cases.length === 0 && review.kind === 'setup' && (
            <p className="citation-review-help">{t('citationReviewNoCases')}</p>
          )}
          {caseSelected && sources.length === 0 && review.kind === 'setup' && (
            <p className="citation-review-help">{t('citationReviewNoSources')}</p>
          )}
        </section>

        {review.kind === 'running' && (
          <p className="citation-review-state">{t('citationReviewLoading')}</p>
        )}
        {review.kind === 'error' && (
          <p className="citation-review-error" role="alert">
            {review.message}
          </p>
        )}
        {message && (
          <p className="citation-review-message" role="status" aria-live="polite">
            {message}
          </p>
        )}

        {loaded && (
          <>
            <section
              className="citation-review-summary"
              aria-labelledby="citation-review-summary-title"
            >
              <div className="citation-review-summary-head">
                <h2 id="citation-review-summary-title">{t('citationReviewSummary')}</h2>
                <span className={`citation-review-run-status run-${loaded.run.status}`}>
                  {t(RUN_STATUS_KEYS[loaded.run.status])}
                </span>
              </div>
              <div className="citation-review-counts">
                <span>
                  {t('citationReviewTotal')}: {total}
                </span>
                <span>
                  {t('citationReviewNeedsAttention')}:{' '}
                  {loaded.items.filter(citationReviewNeedsAttention).length}
                </span>
              </div>
              {relationCounts && (
                <div className="citation-review-relations">
                  {(Object.keys(RELATION_KEYS) as ResearchClaimEvidenceRelation[]).map(
                    (relation) => (
                      <span key={relation}>
                        {t(RELATION_KEYS[relation])}: {relationCounts[relation]}
                      </span>
                    ),
                  )}
                </div>
              )}
              {stale && (
                <p className="citation-review-stale" role="alert">
                  {t('citationReviewStale')}
                </p>
              )}
              {requiresNewReview && (
                <p className="citation-review-stale" role="alert">
                  {t('citationReviewNewReviewRequired')}
                </p>
              )}
              {loaded.run.status === 'failed' && (
                <p className="citation-review-error" role="alert">
                  {t('citationReviewRunError')}
                  {loaded.run.failureStage ? ` · ${loaded.run.failureStage}` : ''}
                  {loaded.run.failureCode ? ` · ${loaded.run.failureCode}` : ''}
                </p>
              )}
            </section>

            <label className="citation-review-filter-label" htmlFor="citation-review-filter">
              {t('citationReviewFilter')}
            </label>
            <select
              id="citation-review-filter"
              className="citation-review-filter"
              value={filter}
              onChange={(event) => setFilter(event.target.value as ReviewFilter)}
            >
              <option value="all">{t('citationReviewAll')}</option>
              <option value="needs">{t('citationReviewNeedsAttention')}</option>
              <option value="supports">{t('citationReviewRelationSupports')}</option>
              <option value="contradicts">{t('citationReviewRelationContradicts')}</option>
              <option value="contextualizes">{t('citationReviewRelationContextualizes')}</option>
              <option value="insufficient">{t('citationReviewRelationInsufficient')}</option>
            </select>

            {filteredItems.length === 0 ? (
              <p className="citation-review-help">{t('citationReviewNoResults')}</p>
            ) : (
              <div className="citation-review-list">
                {filteredItems.map((item, index) => {
                  const relation = itemRelation(item)
                  const sourceLabel = item.sourceId
                    ? (sourceLabels.get(item.sourceId) ?? item.sourceId)
                    : (item.candidates[0]?.sourceLabel ?? null)
                  const showCandidateControls = item.candidates.some(
                    (candidate) =>
                      citationReviewAllowsCandidateConfirmation(item) &&
                      firstVisibleCandidateItemByEntry.get(candidate.resolutionEntryId) ===
                        item.itemId,
                  )
                  return (
                    <article className="citation-review-card" key={item.itemId}>
                      <div className="citation-review-card-head">
                        <span className="citation-review-card-number">#{index + 1}</span>
                        <span className={`citation-review-item-status status-${item.status}`}>
                          {t(STATUS_KEYS[item.status])}
                        </span>
                      </div>
                      <button
                        type="button"
                        className="citation-review-go"
                        disabled={reviewInvalidated}
                        onClick={() => goToCitation(item)}
                      >
                        {t('citationReviewGoToCitation')}
                      </button>
                      <dl className="citation-review-fields">
                        <div>
                          <dt>{t('citationReviewRenderedCitation')}</dt>
                          <dd>{textOrDash(item.renderedText)}</dd>
                        </div>
                        <div>
                          <dt>{t('citationReviewReferenceKey')}</dt>
                          <dd>{textOrDash(item.referenceKey)}</dd>
                        </div>
                        <div>
                          <dt>{t('citationReviewSource')}</dt>
                          <dd>{textOrDash(sourceLabel)}</dd>
                        </div>
                        <div>
                          <dt>{t('citationReviewClaim')}</dt>
                          <dd>{textOrDash(item.claimText)}</dd>
                        </div>
                        <div>
                          <dt>{t('citationReviewManuscriptExcerpt')}</dt>
                          <dd>{textOrDash(item.sourceExcerpt)}</dd>
                        </div>
                      </dl>
                      {relation && (
                        <p className={`citation-review-relation relation-${relation}`}>
                          <strong>{t('citationReviewRelation')}:</strong>{' '}
                          {t(RELATION_KEYS[relation])}
                        </p>
                      )}
                      {item.verification?.rationale && (
                        <p className="citation-review-rationale">
                          <strong>{t('citationReviewRationale')}:</strong>{' '}
                          {item.verification.rationale}
                        </p>
                      )}
                      {item.evidence.length > 0 ? (
                        <section className="citation-review-evidence">
                          <h3>{t('citationReviewCanonicalEvidence')}</h3>
                          {item.evidence.map((evidence) => (
                            <blockquote key={evidence.evidenceId}>
                              <p>{evidence.verbatimExcerpt}</p>
                              <cite>
                                {t('citationReviewLocator')}:{' '}
                                <code>{JSON.stringify(evidence.locator)}</code>
                              </cite>
                            </blockquote>
                          ))}
                        </section>
                      ) : (
                        <p className="citation-review-no-evidence">
                          {t('citationReviewNoEvidence')}
                        </p>
                      )}
                      {item.candidates.length > 0 && (
                        <details className="citation-review-details">
                          <summary>
                            {t('citationReviewCandidates')} ({item.candidates.length})
                          </summary>
                          {item.candidates.map((candidate) => (
                            <div className="citation-review-candidate" key={candidate.candidateId}>
                              <div>
                                <strong>
                                  {textOrDash(
                                    candidate.sourceLabel ?? sourceLabels.get(candidate.sourceId),
                                  )}
                                </strong>
                                <span className="citation-review-candidate-meta">
                                  {textOrDash(candidate.matchKind)} · {candidate.sourceId}
                                </span>
                              </div>
                              {showCandidateControls &&
                                firstVisibleCandidateItemByEntry.get(
                                  candidate.resolutionEntryId,
                                ) === item.itemId &&
                                (confirmedEntries.has(candidate.resolutionEntryId) ? (
                                  <span className="citation-review-confirmed">
                                    {t('citationReviewConfirmed')}
                                  </span>
                                ) : (
                                  <button
                                    type="button"
                                    className="citation-review-confirm"
                                    disabled={
                                      reviewInvalidated ||
                                      busyCandidateId !== null ||
                                      !loaded.run.referenceResolutionRunId
                                    }
                                    onClick={() => void confirmCandidate(candidate)}
                                  >
                                    {busyCandidateId === candidate.candidateId
                                      ? t('citationReviewLoading')
                                      : t('citationReviewConfirm')}
                                  </button>
                                ))}
                            </div>
                          ))}
                        </details>
                      )}
                      <details className="citation-review-details">
                        <summary>{t('citationReviewAudit')}</summary>
                        <dl>
                          <div>
                            <dt>{t('citationReviewDocumentVersion')}</dt>
                            <dd>{loaded.run.documentVersion}</dd>
                          </div>
                          <div>
                            <dt>{t('citationReviewBlock')}</dt>
                            <dd>
                              <code>{item.documentBlockId}</code>
                            </dd>
                          </div>
                          <div>
                            <dt>{t('citationReviewBinding')}</dt>
                            <dd>{textOrDash(item.bindingId)}</dd>
                          </div>
                          <div>
                            <dt>{t('citationReviewBindingMethod')}</dt>
                            <dd>{textOrDash(item.bindingMethod)}</dd>
                          </div>
                          <div>
                            <dt>{t('citationReviewSourceSnapshot')}</dt>
                            <dd>{textOrDash(item.sourceSnapshotId)}</dd>
                          </div>
                          <div>
                            <dt>{t('citationReviewExtraction')}</dt>
                            <dd>{textOrDash(item.extractionId)}</dd>
                          </div>
                          {item.failureCode && (
                            <div>
                              <dt>{t('citationReviewFailureCode')}</dt>
                              <dd>{item.failureCode}</dd>
                            </div>
                          )}
                          {item.verification && (
                            <div>
                              <dt>{t('citationReviewAssessor')}</dt>
                              <dd>
                                {textOrDash(
                                  item.verification.assessorModelId ??
                                    item.verification.assessorProvider,
                                )}
                              </dd>
                            </div>
                          )}
                        </dl>
                      </details>
                    </article>
                  )
                })}
              </div>
            )}
          </>
        )}
      </div>
    </aside>
  )
}
