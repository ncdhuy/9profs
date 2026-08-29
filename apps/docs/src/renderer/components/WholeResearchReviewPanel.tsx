import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { Editor } from '@tiptap/core'
import { TextSelection } from '@tiptap/pm/state'
import type {
  CitationReviewCandidate,
  CoreTransport,
  CoverageAttentionReason,
  CrossClaimConsistencyAttentionReason,
  ManuscriptResearchReviewClaimItem,
  ManuscriptResearchReviewConsistencyClaim,
  ManuscriptResearchReviewConsistencyItem,
  ManuscriptResearchReviewRun,
  ResearchCase,
  ResearchSource,
} from '@genoffice/9profs-core'
import {
  buildManuscriptResearchReviewInput,
} from '../editor/manuscript-citations'
import { findManuscriptClaimRange } from '../editor/research-review-navigation'
import {
  citationReviewAllowsCandidateConfirmation,
} from './CitationReviewPanel'
import { useI18n, type StringKey, type TFunc } from '../i18n/locale'

export interface WholeResearchReviewPanelProps {
  readonly editor: Editor
  readonly documentId: string
  readonly transport: CoreTransport | null
  readonly onClose: () => void
}

type ReviewState =
  | { readonly kind: 'setup' }
  | { readonly kind: 'running' }
  | {
      readonly kind: 'loaded'
      readonly run: ManuscriptResearchReviewRun
      readonly claims: ManuscriptResearchReviewClaimItem[]
      readonly consistency: ManuscriptResearchReviewConsistencyItem[]
    }
  | { readonly kind: 'error'; readonly message: string; readonly run?: ManuscriptResearchReviewRun }

type BusyAction = 'case' | 'source' | 'review' | null
type ReviewTab = 'claims' | 'consistency'
type ClaimFilter = 'all' | 'attention' | 'external' | 'support' | 'contradiction' | 'unavailable'
type ConsistencyFilter = 'all' | 'attention' | 'conflict' | 'unavailable'

const EXPECTATION_KEYS: Record<string, StringKey> = {
  external_evidence_expected: 'researchReviewExpectationExternal',
  external_evidence_context_dependent: 'researchReviewExpectationContext',
  manuscript_internal_support: 'researchReviewExpectationInternal',
  no_external_citation_expected: 'researchReviewExpectationNone',
  uncertain: 'researchReviewExpectationUncertain',
}

const CLAIM_KIND_KEYS: Record<string, StringKey> = {
  external_evidence: 'researchReviewClaimKindExternal',
  manuscript_internal: 'researchReviewClaimKindInternal',
  interpretive: 'researchReviewClaimKindInterpretive',
  non_evidentiary: 'researchReviewClaimKindNonEvidentiary',
  uncertain: 'researchReviewClaimKindUncertain',
}

const COVERAGE_ATTENTION_KEYS: Record<string, StringKey> = {
  no_coverage_attention_detected: 'researchReviewAttentionNone',
  review_suggested: 'researchReviewAttentionReview',
  expectation_review_needed: 'researchReviewAttentionExpectation',
  assessment_unavailable: 'researchReviewAttentionUnavailable',
}

const COVERAGE_REASON_KEYS: Record<CoverageAttentionReason, StringKey> = {
  expected_external_evidence_no_exact_citation_link: 'researchReviewReasonNoExactLink',
  ambiguous_claim_citation_bridge: 'researchReviewReasonAmbiguousBridge',
  citation_verification_blocked: 'researchReviewReasonBlocked',
  citation_verification_incomplete: 'researchReviewReasonIncomplete',
  citation_verification_insufficient: 'researchReviewReasonInsufficient',
  citation_verification_contextualizes: 'researchReviewReasonContextualizes',
  expected_external_evidence_no_supporting_verification: 'researchReviewReasonNoSupporting',
  contradictory_evidence_observed: 'researchReviewReasonContradictory',
  mixed_evidence_relations: 'researchReviewReasonMixed',
  expectation_context_dependent: 'researchReviewReasonContextDependent',
  expectation_uncertain: 'researchReviewReasonUncertain',
  expectation_assessment_failed: 'researchReviewReasonAssessmentFailed',
}

const CONSISTENCY_ATTENTION_KEYS: Record<string, StringKey> = {
  no_internal_consistency_attention_detected: 'researchReviewConsistencyAttentionNone',
  review_suggested: 'researchReviewConsistencyAttentionReview',
  context_review_needed: 'researchReviewConsistencyAttentionContext',
  assessment_unavailable: 'researchReviewConsistencyAttentionUnavailable',
}

const CONSISTENCY_REASON_KEYS: Record<CrossClaimConsistencyAttentionReason, StringKey> = {
  assessed_internal_conflict: 'researchReviewConsistencyReasonConflict',
  quantitative_conflict_observed: 'researchReviewConsistencyReasonQuantitative',
  direction_conflict_observed: 'researchReviewConsistencyReasonDirection',
  modality_conflict_observed: 'researchReviewConsistencyReasonModality',
  causal_strength_conflict_observed: 'researchReviewConsistencyReasonCausal',
  scope_conflict_observed: 'researchReviewConsistencyReasonScope',
  temporal_conflict_observed: 'researchReviewConsistencyReasonTemporal',
  definition_conflict_observed: 'researchReviewConsistencyReasonDefinition',
  propositional_conflict_observed: 'researchReviewConsistencyReasonProposition',
  consistency_context_insufficient: 'researchReviewConsistencyReasonContext',
  consistency_assessment_failed: 'researchReviewConsistencyReasonFailed',
}

const CONSISTENCY_RELATION_KEYS: Record<string, StringKey> = {
  conflict: 'researchReviewConsistencyConflict',
  compatible: 'researchReviewConsistencyCompatible',
  qualification_or_refinement: 'researchReviewConsistencyQualification',
  equivalent_or_restatement: 'researchReviewConsistencyEquivalent',
  not_meaningfully_comparable: 'researchReviewConsistencyNotComparable',
  insufficient_context: 'researchReviewConsistencyInsufficient',
}

const DIMENSION_KEYS: Record<string, StringKey> = {
  proposition: 'researchReviewDimensionProposition',
  quantitative: 'researchReviewDimensionQuantitative',
  direction: 'researchReviewDimensionDirection',
  modality_or_certainty: 'researchReviewDimensionModality',
  causal_strength: 'researchReviewDimensionCausal',
  scope_or_population: 'researchReviewDimensionScope',
  temporal: 'researchReviewDimensionTemporal',
  definition: 'researchReviewDimensionDefinition',
  other: 'researchReviewDimensionOther',
}

const CITATION_STATUS_KEYS: Record<string, StringKey> = {
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

const VERIFICATION_STATUS_KEYS: Record<string, StringKey> = {
  running: 'citationReviewStatusVerificationRunning',
  completed: 'citationReviewStatusVerificationCompleted',
  failed: 'citationReviewStatusVerificationFailed',
}

function label(t: TFunc, keys: Record<string, StringKey>, value: string | null | undefined): string {
  return value === null || value === undefined
    ? t('researchReviewUnknown')
    : t(keys[value] ?? 'researchReviewUnknown')
}

function errorMessage(fallback: string): string {
  return fallback
}

function locatorText(t: TFunc, locator: { readonly kind: string; readonly [key: string]: unknown }): string {
  switch (locator.kind) {
    case 'pdf':
      return `${t('researchReviewPage')} ${String(locator.page)}`
    case 'pdf_text_range':
      return `${t('researchReviewPage')} ${String(locator.page)} · ${t('researchReviewRange')} ${String(locator.start)}–${String(locator.end)}`
    case 'text_range':
      return `${t('researchReviewRange')} ${String(locator.start)}–${String(locator.end)}`
    case 'manuscript':
      return `${String(locator.block_id)} · ${String(locator.start ?? '—')}–${String(locator.end ?? '—')}`
    case 'spreadsheet':
      return `${String(locator.sheet)} · ${String(locator.range)}`
    case 'web':
      return String(locator.fragment ?? '') || t('researchReviewUnknown')
    case 'regulation':
      return `${String(locator.article)}${locator.section ? ` · ${String(locator.section)}` : ''}`
    default:
      return t('researchReviewUnknown')
  }
}

function runStatusLabel(t: TFunc, status: ManuscriptResearchReviewRun['status']): string {
  return t(
    status === 'running'
      ? 'researchReviewRunRunning'
      : status === 'completed'
        ? 'researchReviewRunCompleted'
        : 'researchReviewRunStatusFailed',
  )
}

function Count({ label: text, value }: { readonly label: string; readonly value: number }) {
  return (
    <div className="whole-research-review-count">
      <dt>{text}</dt>
      <dd>{value}</dd>
    </div>
  )
}

function ClaimExcerpt({
  t,
  claim,
}: {
  readonly t: TFunc
  readonly claim: ManuscriptResearchReviewClaimItem | ManuscriptResearchReviewConsistencyClaim
}) {
  return (
    <>
      <div className="whole-research-review-field-label">{t('researchReviewManuscriptText')}</div>
      <blockquote className="whole-research-review-excerpt">{claim.sourceExcerpt}</blockquote>
      <div className="whole-research-review-field-label">{t('researchReviewNormalizedClaim')}</div>
      <p className="whole-research-review-claim-text">{claim.claimText}</p>
    </>
  )
}

export function WholeResearchReviewPanel({
  editor,
  documentId,
  transport,
  onClose,
}: WholeResearchReviewPanelProps) {
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
  const [message, setMessage] = useState<string | null>(null)
  const [currentVersion, setCurrentVersion] = useState<number | null>(null)
  const [reviewTab, setReviewTab] = useState<ReviewTab>('claims')
  const [claimFilter, setClaimFilter] = useState<ClaimFilter>('all')
  const [consistencyFilter, setConsistencyFilter] = useState<ConsistencyFilter>('all')
  const [confirmedEntries, setConfirmedEntries] = useState<Set<string>>(new Set())
  const [busyCandidateId, setBusyCandidateId] = useState<string | null>(null)
  const [requiresNewReview, setRequiresNewReview] = useState(false)
  const resolutionRunIdRef = useRef<string | null>(null)

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
    resolutionRunIdRef.current = null
    if (!transport) {
      setReview({ kind: 'error', message: translateRef.current('researchReviewCoreUnavailable') })
      return () => {
        disposed = true
      }
    }
    void transport
      .researchCases()
      .then((next) => {
        if (!disposed) setCases(next)
      })
      .catch(() => {
        if (!disposed) {
          setMessage(errorMessage(translateRef.current('researchReviewUnavailable')))
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
        setSources(next.filter((source) => source.researchCaseId === caseId && source.kind === 'manuscript'))
      })
      .catch(() => {
        if (!disposed) setMessage(translateRef.current('researchReviewSourcesUnavailable'))
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
    } catch {
      setMessage(translateRef.current('researchReviewCreateFailed'))
    } finally {
      setBusy(null)
    }
  }

  const createSource = async () => {
    const labelText = newSourceLabel.trim()
    if (!transport || !caseId || !labelText || busy) return
    setBusy('source')
    setMessage(null)
    try {
      const created = await transport.createResearchSource({
        researchCaseId: caseId,
        kind: 'manuscript',
        label: labelText,
      })
      setSources((current) => [...current, created])
      setSourceId(created.sourceId)
      setNewSourceLabel('')
    } catch {
      setMessage(translateRef.current('researchReviewCreateFailed'))
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
    setConfirmedEntries(new Set())
    resolutionRunIdRef.current = null
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
      const input = buildManuscriptResearchReviewInput({
        editor,
        activeDocument: active,
        manuscriptSourceId: sourceId,
      })
      const run = await transport.startManuscriptResearchReview(caseId, input)
      if (run.status === 'failed') {
        setReview({
          kind: 'error',
          message: translateRef.current('researchReviewRunFailed'),
          run,
        })
        return
      }
      if (run.status !== 'completed') {
        setReview({ kind: 'error', message: translateRef.current('researchReviewUnavailable'), run })
        return
      }
      const [claims, consistency] = await Promise.all([
        transport.manuscriptResearchReviewClaims(run.reviewRunId),
        transport.manuscriptResearchReviewConsistency(run.reviewRunId),
      ])
      setReview({ kind: 'loaded', run, claims, consistency })
      setReviewTab('claims')
    } catch {
      setReview({ kind: 'error', message: translateRef.current('researchReviewUnavailable') })
    } finally {
      setBusy(null)
    }
  }, [busy, caseId, documentId, editor, sourceId, transport])

  const loaded = review.kind === 'loaded' ? review : null

  useEffect(() => {
    if (!transport || !loaded) return
    let disposed = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const refreshVersion = () => {
      if (timer !== undefined) clearTimeout(timer)
      timer = setTimeout(() => {
        setCurrentVersion(null)
        void transport
          .activeDocument(documentId)
          .then((active) => {
            if (
              !disposed &&
              active.documentId === documentId &&
              active.availability === 'available' &&
              Number.isInteger(active.version) &&
              active.version >= 0
            ) {
              setCurrentVersion(active.version)
            }
          })
          .catch(() => undefined)
      }, 250)
    }
    editor.on('update', refreshVersion)
    return () => {
      disposed = true
      if (timer !== undefined) clearTimeout(timer)
      editor.off('update', refreshVersion)
    }
  }, [documentId, editor, loaded, transport])

  const stale = loaded !== null && (currentVersion === null || currentVersion !== loaded.run.documentVersion)
  const reviewInvalidated = stale || requiresNewReview
  const caseSelected = cases.some((researchCase) => researchCase.caseId === caseId)
  const sourceSelected = sources.some((source) => source.sourceId === sourceId)

  const filteredClaims = useMemo(() => {
    if (!loaded) return []
    return loaded.claims.filter((claim) => {
      switch (claimFilter) {
        case 'attention':
          return claim.attentionState !== 'no_coverage_attention_detected'
        case 'external':
          return claim.expectation === 'external_evidence_expected'
        case 'support':
          return claim.supportCount > 0
        case 'contradiction':
          return claim.contradictionCount > 0
        case 'unavailable':
          return claim.attentionState === 'assessment_unavailable' || claim.assessmentStatus === 'assessment_failed'
        default:
          return true
      }
    })
  }, [claimFilter, loaded])

  const filteredConsistency = useMemo(() => {
    if (!loaded) return []
    return loaded.consistency.filter((item) => {
      switch (consistencyFilter) {
        case 'attention':
          return item.attentionState !== 'no_internal_consistency_attention_detected'
        case 'conflict':
          return item.relation === 'conflict'
        case 'unavailable':
          return item.assessmentStatus === 'assessment_failed' || item.attentionState === 'assessment_unavailable'
        default:
          return true
      }
    })
  }, [consistencyFilter, loaded])

  const firstCandidateTargetByEntry = useMemo(() => {
    const result = new Map<string, string>()
    for (const claim of filteredClaims) {
      for (const target of claim.targets) {
        const targetKey = `${claim.inventoryItemId}:${target.citationTargetId}`
        for (const candidate of target.citationReviewItem.candidates) {
          if (
            citationReviewAllowsCandidateConfirmation(target.citationReviewItem) &&
            !result.has(candidate.resolutionEntryId)
          ) {
            result.set(candidate.resolutionEntryId, targetKey)
          }
        }
      }
    }
    return result
  }, [filteredClaims])

  const navigateToClaim = useCallback(
    (claim: ManuscriptResearchReviewClaimItem | ManuscriptResearchReviewConsistencyClaim) => {
      if (reviewInvalidated) {
        setMessage(
          translateRef.current(
            requiresNewReview ? 'researchReviewNewReviewRequired' : 'researchReviewStale',
          ),
        )
        return
      }
      const range = findManuscriptClaimRange(editor, claim)
      if (range === null) {
        setMessage(translateRef.current('researchReviewNavigationFailed'))
        return
      }
      editor.view.dispatch(
        editor.state.tr
          .setSelection(TextSelection.create(editor.state.doc, range.from, range.to))
          .scrollIntoView(),
      )
      editor.commands.focus()
    },
    [editor, requiresNewReview, reviewInvalidated],
  )

  const confirmCandidate = async (candidate: CitationReviewCandidate, item: ManuscriptResearchReviewClaimItem['targets'][number]) => {
    if (!transport || !loaded || reviewInvalidated || !item.citationReviewItem.resolutionEntryId) {
      setMessage(
        translateRef.current(
          requiresNewReview ? 'researchReviewNewReviewRequired' : 'researchReviewConfirmationBlocked',
        ),
      )
      return
    }
    if (!citationReviewAllowsCandidateConfirmation(item.citationReviewItem)) return
    const entryId = candidate.resolutionEntryId
    if (confirmedEntries.has(entryId)) return
    setBusyCandidateId(candidate.candidateId)
    setMessage(null)
    try {
      let resolutionRunId = resolutionRunIdRef.current
      if (!resolutionRunId) {
        if (!loaded.run.citationReviewRunId) throw new Error('citation review unavailable')
        const citationRun = await transport.manuscriptCitationReview(loaded.run.citationReviewRunId)
        if (!citationRun.referenceResolutionRunId) throw new Error('resolution run unavailable')
        resolutionRunId = citationRun.referenceResolutionRunId
        resolutionRunIdRef.current = resolutionRunId
      }
      await transport.confirmManuscriptReferenceCandidate(resolutionRunId, entryId, candidate.candidateId)
      setConfirmedEntries((current) => new Set(current).add(entryId))
      setRequiresNewReview(true)
      setMessage(translateRef.current('researchReviewConfirmed'))
    } catch {
      setMessage(translateRef.current('researchReviewConfirmationFailed'))
    } finally {
      setBusyCandidateId(null)
    }
  }

  const renderTarget = (claim: ManuscriptResearchReviewClaimItem, target: ManuscriptResearchReviewClaimItem['targets'][number]) => {
    const targetKey = `${claim.inventoryItemId}:${target.citationTargetId}`
    return (
      <div className="whole-research-review-target" key={target.citationTargetId}>
        <div className="whole-research-review-card-head">
          <strong>{t('researchReviewCitationEvidenceDetails')}</strong>
          <span className={`whole-research-review-status status-${target.reviewStatus}`}>
            {label(t, CITATION_STATUS_KEYS, target.reviewStatus)}
          </span>
        </div>
        <dl className="whole-research-review-fields">
          <div>
            <dt>{t('researchReviewSource')}</dt>
            <dd>{target.sourceId ?? t('researchReviewUnknown')}</dd>
          </div>
          <div>
            <dt>{t('citationReviewReferenceKey')}</dt>
            <dd>{target.citationReviewItem.referenceKey}</dd>
          </div>
          <div>
            <dt>{t('citationReviewLocator')}</dt>
            <dd>{target.citationReviewItem.citedLocator ?? t('researchReviewUnknown')}</dd>
          </div>
          <div>
            <dt>{t('researchReviewVerification')}</dt>
            <dd>{label(t, VERIFICATION_STATUS_KEYS, target.verificationStatus)}</dd>
          </div>
          <div>
            <dt>{t('researchReviewRationale')}</dt>
            <dd>{target.rationale ?? t('researchReviewUnknown')}</dd>
          </div>
        </dl>
        <div className="whole-research-review-relation">
          {target.relation
            ? label(t, {
                supports: 'researchReviewRelationSupports',
                contradicts: 'researchReviewRelationContradicts',
                contextualizes: 'researchReviewRelationContextualizes',
                insufficient: 'researchReviewRelationInsufficient',
              }, target.relation)
            : t('researchReviewAssessmentUnavailable')}
        </div>
        <div className="whole-research-review-evidence">
          <h4>{t('researchReviewEvidencePassage')}</h4>
          {target.evidence.length === 0 ? (
            <p>{t('researchReviewNoEvidence')}</p>
          ) : (
            target.evidence.map((evidence) => (
              <blockquote key={evidence.evidenceId}>
                <p>{evidence.verbatimExcerpt}</p>
                <cite>{locatorText(t, evidence.locator)}</cite>
              </blockquote>
            ))
          )}
        </div>
        {target.citationReviewItem.candidates.length > 0 && (
          <details className="whole-research-review-details">
            <summary>{t('researchReviewCandidates')} ({target.citationReviewItem.candidates.length})</summary>
            {target.citationReviewItem.candidates.map((candidate) => {
              const canConfirm =
                !confirmedEntries.has(candidate.resolutionEntryId) &&
                firstCandidateTargetByEntry.get(candidate.resolutionEntryId) === targetKey &&
                citationReviewAllowsCandidateConfirmation(target.citationReviewItem)
              return (
                <div className="whole-research-review-candidate" key={candidate.candidateId}>
                  <div>
                    <strong>{candidate.sourceLabel ?? candidate.sourceId}</strong>
                    <span>{candidate.matchKind ?? t('researchReviewUnknown')}</span>
                  </div>
                  {canConfirm ? (
                    <button
                      type="button"
                      className="whole-research-review-confirm"
                      disabled={reviewInvalidated || busyCandidateId !== null}
                      onClick={() => void confirmCandidate(candidate, target)}
                    >
                      {busyCandidateId === candidate.candidateId
                        ? t('researchReviewLoading')
                        : t('researchReviewConfirm')}
                    </button>
                  ) : confirmedEntries.has(candidate.resolutionEntryId) ? (
                    <span className="whole-research-review-muted">{t('researchReviewConfirmedNeedsRerun')}</span>
                  ) : null}
                </div>
              )
            })}
          </details>
        )}
        <details className="whole-research-review-details">
          <summary>{t('researchReviewAudit')}</summary>
          <dl className="whole-research-review-fields">
            <div><dt>{t('researchReviewBinding')}</dt><dd>{target.bindingId ?? t('researchReviewUnknown')}</dd></div>
            <div><dt>{t('researchReviewSourceSnapshot')}</dt><dd>{target.sourceSnapshotId ?? t('researchReviewUnknown')}</dd></div>
            <div><dt>{t('researchReviewExtraction')}</dt><dd>{target.extractionId ?? t('researchReviewUnknown')}</dd></div>
            <div><dt>{t('researchReviewFailureCode')}</dt><dd>{target.failureCode ?? target.verificationFailureCode ?? t('researchReviewUnknown')}</dd></div>
          </dl>
        </details>
      </div>
    )
  }

  const renderClaim = (claim: ManuscriptResearchReviewClaimItem) => (
    <article className="whole-research-review-card" key={claim.inventoryItemId}>
      <div className="whole-research-review-card-head">
        <span className="whole-research-review-card-number">#{claim.ordinal}</span>
        <span className="whole-research-review-status">{label(t, CLAIM_KIND_KEYS, claim.claimReviewKind)}</span>
      </div>
      <ClaimExcerpt t={t} claim={claim} />
      <dl className="whole-research-review-fields">
        <div><dt>{t('researchReviewExpectation')}</dt><dd>{label(t, EXPECTATION_KEYS, claim.expectation)}</dd></div>
        <div><dt>{t('researchReviewExpectationAssessment')}</dt><dd>{claim.assessmentStatus === 'assessed' ? t('researchReviewAssessed') : t('researchReviewAssessmentFailed')}</dd></div>
        <div><dt>{t('researchReviewAttention')}</dt><dd>{label(t, COVERAGE_ATTENTION_KEYS, claim.attentionState)}</dd></div>
        <div><dt>{t('researchReviewRationale')}</dt><dd>{claim.expectationRationale ?? t('researchReviewUnknown')}</dd></div>
      </dl>
      {claim.attentionReasons.length > 0 && (
        <ul className="whole-research-review-reasons">
          {claim.attentionReasons.map((reason) => <li key={reason}>{label(t, COVERAGE_REASON_KEYS, reason)}</li>)}
        </ul>
      )}
      <dl className="whole-research-review-counts">
        {claim.supportCount > 0 && <Count label={t('researchReviewSupports')} value={claim.supportCount} />}
        {claim.contradictionCount > 0 && <Count label={t('researchReviewContradicts')} value={claim.contradictionCount} />}
        {claim.contextualizeCount > 0 && <Count label={t('researchReviewContextualizes')} value={claim.contextualizeCount} />}
        {claim.insufficientCount > 0 && <Count label={t('researchReviewInsufficient')} value={claim.insufficientCount} />}
        {claim.blockedCount > 0 && <Count label={t('researchReviewBlocked')} value={claim.blockedCount} />}
        {claim.unverifiedCount > 0 && <Count label={t('researchReviewUnverified')} value={claim.unverifiedCount} />}
      </dl>
      <button
        type="button"
        className="whole-research-review-go"
        disabled={reviewInvalidated}
        onClick={() => navigateToClaim(claim)}
      >
        {t('researchReviewGoToClaimA')}
      </button>
      {claim.targets.length > 0 && (
        <details className="whole-research-review-details">
          <summary>{t('researchReviewCitationEvidenceDetails')} ({claim.targets.length})</summary>
          {claim.targets.map((target) => renderTarget(claim, target))}
        </details>
      )}
      <details className="whole-research-review-details">
        <summary>{t('researchReviewAudit')}</summary>
        <dl className="whole-research-review-fields">
          <div><dt>{t('researchReviewDocument')}</dt><dd>{claim.documentBlockId}</dd></div>
          <div><dt>{t('researchReviewFailureCode')}</dt><dd>{claim.assessmentStatus === 'assessment_failed' ? t('researchReviewAssessmentFailed') : t('researchReviewUnknown')}</dd></div>
        </dl>
      </details>
    </article>
  )

  const renderConsistency = (item: ManuscriptResearchReviewConsistencyItem) => (
    <article className="whole-research-review-card" key={item.assessmentItemId}>
      <div className={`whole-research-review-card-head${item.relation === 'conflict' ? ' is-conflict' : ''}`}>
        <span className="whole-research-review-status">
          {label(t, CONSISTENCY_RELATION_KEYS, item.relation)}
        </span>
        <span className="whole-research-review-status">
          {item.assessmentStatus === 'assessed' ? t('researchReviewAssessed') : t('researchReviewAssessmentFailed')}
        </span>
      </div>
      <div className="whole-research-review-pair">
        <div>
          <span className="whole-research-review-field-label">{t('researchReviewGoToClaimA')}</span>
          <ClaimExcerpt t={t} claim={item.left} />
          <button type="button" className="whole-research-review-go" disabled={reviewInvalidated} onClick={() => navigateToClaim(item.left)}>
            {t('researchReviewGoToClaimA')}
          </button>
        </div>
        <div>
          <span className="whole-research-review-field-label">{t('researchReviewGoToClaimB')}</span>
          <ClaimExcerpt t={t} claim={item.right} />
          <button type="button" className="whole-research-review-go" disabled={reviewInvalidated} onClick={() => navigateToClaim(item.right)}>
            {t('researchReviewGoToClaimB')}
          </button>
        </div>
      </div>
      <div className="whole-research-review-field-label">{t('researchReviewDimensions')}</div>
      <div className="whole-research-review-dimensions">
        {item.dimensions.map((dimension) => <span key={dimension}>{label(t, DIMENSION_KEYS, dimension)}</span>)}
      </div>
      <p className="whole-research-review-rationale">{item.rationale ?? t('researchReviewUnknown')}</p>
      {item.attentionReasons.length > 0 && (
        <ul className="whole-research-review-reasons">
          {item.attentionReasons.map((reason) => <li key={reason}>{label(t, CONSISTENCY_REASON_KEYS, reason)}</li>)}
        </ul>
      )}
      <details className="whole-research-review-details">
        <summary>{t('researchReviewAudit')}</summary>
        <dl className="whole-research-review-fields">
          <div><dt>{t('researchReviewFailureCode')}</dt><dd>{item.failureCode ?? t('researchReviewUnknown')}</dd></div>
          <div><dt>{t('researchReviewAttention')}</dt><dd>{label(t, CONSISTENCY_ATTENTION_KEYS, item.attentionState)}</dd></div>
        </dl>
      </details>
    </article>
  )

  return (
    <aside className="comments-pane whole-research-review-pane" aria-label={t('researchReviewTitle')}>
      <header className="comments-pane-head">
        <div>
          <div className="comments-pane-title">{t('researchReviewTitle')}</div>
          <span className="citation-review-mode">{t('researchReviewReadOnly')}</span>
        </div>
        <button type="button" className="comments-pane-close" aria-label={t('researchReviewClose')} onClick={onClose}>
          ×
        </button>
      </header>
      <div className="whole-research-review-scroll">
        <section className="whole-research-review-context">
          <h2>{t('researchReviewContext')}</h2>
          <label htmlFor="whole-research-review-case">{t('researchReviewCase')}</label>
          <select
            id="whole-research-review-case"
            value={caseId}
            disabled={busy !== null || !transport}
            onChange={(event) => {
              setCaseId(event.target.value)
              setReview({ kind: 'setup' })
              setMessage(null)
            }}
          >
            <option value="">{t('researchReviewSelectCase')}</option>
            {cases.map((researchCase) => <option key={researchCase.caseId} value={researchCase.caseId}>{researchCase.title}</option>)}
          </select>
          <div className="citation-review-create-row">
            <input
              value={newCaseTitle}
              disabled={busy !== null || !transport}
              aria-label={t('researchReviewNewCase')}
              placeholder={t('researchReviewNewCase')}
              onChange={(event) => setNewCaseTitle(event.target.value)}
              onKeyDown={(event) => { if (event.key === 'Enter') void createCase() }}
            />
            <button type="button" className="btn-ghost" disabled={busy !== null || !transport || !newCaseTitle.trim()} onClick={() => void createCase()}>
              {t('researchReviewCreateCase')}
            </button>
          </div>
          {!cases.length && <p className="whole-research-review-help">{t('researchReviewNoCases')}</p>}

          <label htmlFor="whole-research-review-source">{t('researchReviewManuscriptSource')}</label>
          <select
            id="whole-research-review-source"
            value={sourceId}
            disabled={busy !== null || !caseSelected}
            onChange={(event) => {
              setSourceId(event.target.value)
              setReview({ kind: 'setup' })
              setMessage(null)
            }}
          >
            <option value="">{t('researchReviewSelectSource')}</option>
            {sources.map((source) => <option key={source.sourceId} value={source.sourceId}>{source.label}</option>)}
          </select>
          <div className="citation-review-create-row">
            <input
              value={newSourceLabel}
              disabled={busy !== null || !caseSelected || !transport}
              aria-label={t('researchReviewNewSource')}
              placeholder={t('researchReviewNewSource')}
              onChange={(event) => setNewSourceLabel(event.target.value)}
              onKeyDown={(event) => { if (event.key === 'Enter') void createSource() }}
            />
            <button type="button" className="btn-ghost" disabled={busy !== null || !caseSelected || !transport || !newSourceLabel.trim()} onClick={() => void createSource()}>
              {t('researchReviewCreateSource')}
            </button>
          </div>
          {caseSelected && !sources.length && <p className="whole-research-review-help">{t('researchReviewNoSources')}</p>}
          <button
            type="button"
            className="citation-review-start"
            disabled={busy !== null || !caseSelected || !sourceSelected || !transport}
            onClick={() => void startReview()}
          >
            {loaded || review.kind === 'error' ? t('researchReviewRerun') : t('researchReviewRun')}
          </button>
        </section>

        {message && <p className="whole-research-review-message" aria-live="polite">{message}</p>}
        {review.kind === 'running' && (
          <div className="whole-research-review-state" aria-live="polite">
            <p>{t('researchReviewLoading')}</p>
            <p>{t('researchReviewAnalyzing')}</p>
          </div>
        )}
        {review.kind === 'error' && (
          <section className="whole-research-review-error" aria-live="assertive">
            <p>{review.message}</p>
            {review.run && (
              <dl className="whole-research-review-fields">
                <div><dt>{t('researchReviewRunStatusFailed')}</dt><dd>{runStatusLabel(t, review.run.status)}</dd></div>
                <div><dt>{t('researchReviewFailureStage')}</dt><dd>{review.run.failureStage ?? t('researchReviewUnknown')}</dd></div>
                <div><dt>{t('researchReviewFailureCode')}</dt><dd>{review.run.failureCode ?? t('researchReviewUnknown')}</dd></div>
              </dl>
            )}
          </section>
        )}
        {loaded && (
          <>
            <section className="whole-research-review-overview">
              <h2>{t('researchReviewOverview')}</h2>
              {stale && <p className="whole-research-review-stale" aria-live="polite">{t('researchReviewStale')}</p>}
              {requiresNewReview && <p className="whole-research-review-message" aria-live="polite">{t('researchReviewNewReviewRequired')}</p>}
              <div className="whole-research-review-axis-grid">
                <div>
                  <h3>{t('researchReviewEvidenceCoverage')}</h3>
                  <dl className="whole-research-review-counts">
                    <Count label={t('researchReviewTotalInventoryClaims')} value={loaded.run.summary?.totalInventoryClaims ?? 0} />
                    <Count label={t('researchReviewNeedsAttention')} value={loaded.run.summary?.coverageReviewSuggestedCount ?? 0} />
                    <Count label={t('researchReviewExternalExpected')} value={loaded.run.summary?.expectationReviewNeededCount ?? 0} />
                    <Count label={t('researchReviewAssessmentUnavailable')} value={loaded.run.summary?.assessmentUnavailableCount ?? 0} />
                    <Count label={t('researchReviewSupportObserved')} value={loaded.run.summary?.claimsWithSupportCount ?? 0} />
                    <Count label={t('researchReviewContradictionObserved')} value={loaded.run.summary?.claimsWithContradictionCount ?? 0} />
                    <Count label={t('researchReviewBlocked')} value={loaded.run.summary?.claimsWithBlockedVerificationCount ?? 0} />
                    <Count label={t('researchReviewUnverified')} value={loaded.run.summary?.claimsWithUnverifiedVerificationCount ?? 0} />
                  </dl>
                </div>
                <div>
                  <h3>{t('researchReviewInternalConsistency')}</h3>
                  <dl className="whole-research-review-counts">
                    <Count label={t('researchReviewAssessedPairs')} value={loaded.run.summary?.consistencyAssessedCount ?? 0} />
                    <Count label={t('researchReviewConsistencyConflict')} value={loaded.run.summary?.consistencyConflictCount ?? 0} />
                    <Count label={t('researchReviewConsistencyCompatible')} value={loaded.run.summary?.consistencyCompatibleCount ?? 0} />
                    <Count label={t('researchReviewConsistencyQualification')} value={loaded.run.summary?.consistencyQualificationCount ?? 0} />
                    <Count label={t('researchReviewConsistencyEquivalent')} value={loaded.run.summary?.consistencyEquivalentCount ?? 0} />
                    <Count label={t('researchReviewConsistencyNotComparable')} value={loaded.run.summary?.consistencyNotComparableCount ?? 0} />
                    <Count label={t('researchReviewConsistencyInsufficient')} value={loaded.run.summary?.consistencyInsufficientContextCount ?? 0} />
                    <Count label={t('researchReviewAssessmentUnavailable')} value={loaded.run.summary?.consistencyAssessmentFailureCount ?? 0} />
                  </dl>
                </div>
              </div>
              <dl className="whole-research-review-fields">
                <div><dt>{t('researchReviewCoverageScope')}</dt><dd>{loaded.run.summary?.coverageScope ?? t('researchReviewUnknown')}</dd></div>
                <div><dt>{t('researchReviewCoverageLimitations')}</dt><dd>{loaded.run.summary?.coverageLimitations.join(' · ') || t('researchReviewUnknown')}</dd></div>
              </dl>
              <p className="whole-research-review-limitation">{t('researchReviewGlobalLimitation')}</p>
              <details className="whole-research-review-details">
                <summary>{t('researchReviewAnalysisCoverage')}</summary>
                <dl className="whole-research-review-counts">
                  <Count label={t('researchReviewCandidateClaimCount')} value={loaded.run.summary?.candidateClaimCount ?? 0} />
                  <Count label={t('researchReviewCandidateBatchCount')} value={loaded.run.summary?.candidateBatchCount ?? 0} />
                  <Count label={t('researchReviewCandidateExpectedWindowCount')} value={loaded.run.summary?.candidateExpectedWindowCount ?? 0} />
                  <Count label={t('researchReviewCandidateProcessedWindowCount')} value={loaded.run.summary?.candidateProcessedWindowCount ?? 0} />
                  <Count label={t('researchReviewCandidatePairCount')} value={loaded.run.summary?.candidatePairCount ?? 0} />
                </dl>
                <p className="whole-research-review-help">{t('researchReviewCandidateCoverageWarning')}</p>
              </details>
            </section>
            {loaded.claims.length === 0 && (
              <p className="whole-research-review-help">{t('researchReviewNoClaims')}</p>
            )}
            <div className="whole-research-review-tabs" role="tablist" aria-label={t('researchReviewTitle')}>
              <button type="button" role="tab" aria-selected={reviewTab === 'claims'} onClick={() => setReviewTab('claims')}>
                {t('researchReviewClaims')} ({loaded.claims.length})
              </button>
              <button type="button" role="tab" aria-selected={reviewTab === 'consistency'} onClick={() => setReviewTab('consistency')}>
                {t('researchReviewConsistency')} ({loaded.consistency.length})
              </button>
            </div>
            {reviewTab === 'claims' ? (
              <section>
                <label className="whole-research-review-filter-label" htmlFor="whole-research-review-claim-filter">{t('researchReviewFilter')}</label>
                <select id="whole-research-review-claim-filter" className="whole-research-review-filter" value={claimFilter} onChange={(event) => setClaimFilter(event.target.value as ClaimFilter)}>
                  <option value="all">{t('researchReviewAll')}</option>
                  <option value="attention">{t('researchReviewNeedsAttention')}</option>
                  <option value="external">{t('researchReviewExternalExpected')}</option>
                  <option value="support">{t('researchReviewSupportObserved')}</option>
                  <option value="contradiction">{t('researchReviewContradictionObserved')}</option>
                  <option value="unavailable">{t('researchReviewAssessmentUnavailable')}</option>
                </select>
                {!filteredClaims.length ? <p className="whole-research-review-help">{t('researchReviewNoResults')}</p> : <div className="whole-research-review-list">{filteredClaims.map(renderClaim)}</div>}
              </section>
            ) : (
              <section>
                <label className="whole-research-review-filter-label" htmlFor="whole-research-review-consistency-filter">{t('researchReviewFilter')}</label>
                <select id="whole-research-review-consistency-filter" className="whole-research-review-filter" value={consistencyFilter} onChange={(event) => setConsistencyFilter(event.target.value as ConsistencyFilter)}>
                  <option value="all">{t('researchReviewAll')}</option>
                  <option value="attention">{t('researchReviewNeedsAttention')}</option>
                  <option value="conflict">{t('researchReviewConsistencyConflict')}</option>
                  <option value="unavailable">{t('researchReviewAssessmentUnavailable')}</option>
                </select>
                {!filteredConsistency.length ? <p className="whole-research-review-help">{t('researchReviewNoResults')}</p> : <div className="whole-research-review-list">{filteredConsistency.map(renderConsistency)}</div>}
              </section>
            )}
            <details className="whole-research-review-details whole-research-review-audit">
              <summary>{t('researchReviewAudit')}</summary>
              <dl className="whole-research-review-fields">
                <div><dt>{t('researchReviewRunId')}</dt><dd>{loaded.run.reviewRunId}</dd></div>
                <div><dt>{t('researchReviewDocument')}</dt><dd>{loaded.run.documentId} · v{loaded.run.documentVersion}</dd></div>
                <div><dt>{t('researchReviewCitationReviewRunId')}</dt><dd>{loaded.run.citationReviewRunId ?? t('researchReviewUnknown')}</dd></div>
                <div><dt>{t('researchReviewClaimInventoryRunId')}</dt><dd>{loaded.run.claimInventoryRunId ?? t('researchReviewUnknown')}</dd></div>
                <div><dt>{t('researchReviewClaimCoverageRunId')}</dt><dd>{loaded.run.claimCoverageRunId ?? t('researchReviewUnknown')}</dd></div>
                <div><dt>{t('researchReviewCitationExpectationRunId')}</dt><dd>{loaded.run.citationExpectationRunId ?? t('researchReviewUnknown')}</dd></div>
                <div><dt>{t('researchReviewCrossClaimCandidateRunId')}</dt><dd>{loaded.run.crossClaimCandidateRunId ?? t('researchReviewUnknown')}</dd></div>
                <div><dt>{t('researchReviewCrossClaimAssessmentRunId')}</dt><dd>{loaded.run.crossClaimAssessmentRunId ?? t('researchReviewUnknown')}</dd></div>
                <div><dt>{t('researchReviewInputHash')}</dt><dd>{loaded.run.inputHash}</dd></div>
                <div><dt>{t('researchReviewExecutionIdentityHash')}</dt><dd>{loaded.run.executionIdentityHash ?? t('researchReviewUnknown')}</dd></div>
                <div><dt>{t('researchReviewContractVersion')}</dt><dd>{loaded.run.reviewContractVersion}</dd></div>
              </dl>
            </details>
          </>
        )}
      </div>
    </aside>
  )
}
