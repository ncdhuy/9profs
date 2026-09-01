import { useEffect, useState } from 'react'
import type { Editor } from '@tiptap/core'
import type {
  CoreTransport,
  ManuscriptReviewAuthorityReference,
  ManuscriptReviewFinding,
  ManuscriptReviewLocator,
  ManuscriptReviewResult,
  ResearchContext,
} from '@genoffice/9profs-core'
import { navigateToManuscriptReviewLocation } from '../editor/manuscript-review-navigation'
import { useI18n } from '../i18n/locale'

export interface ManuscriptReviewPanelProps {
  readonly editor: Editor
  readonly documentId: string
  readonly transport: CoreTransport | null
  readonly onClose: () => void
}

type ReviewState =
  | { kind: 'idle' }
  | { kind: 'running' }
  | { kind: 'loaded'; result: ManuscriptReviewResult }
  | { kind: 'error'; message: string }

// The product has no stored project context seam yet. Keep the invocation
// explicit and generic; this is the known manuscript context for the MVP dogfood.
const MVP_RESEARCH_CONTEXT: ResearchContext = {
  language: 'vi',
  researchFamilies: ['MED'],
  artifactType: 'master_thesis',
  academicLevel: 'master',
  studyDesigns: [],
  reportingGuidelines: [],
  organization: 'hiu',
}

const AUTHORITY_LABELS: Record<string, string> = {
  'research.core': 'Research principles',
  'domain.med': 'Medicine & Health Sciences',
  'artifact.master-thesis': "Master's thesis standards",
  'editorial.vi': 'Vietnamese academic writing',
}

function locationLabel(locator: ManuscriptReviewLocator): string {
  if (locator.sectionId) {
    return `Section ${locator.sectionId.replace(/^section:/, '')}`
  }
  return `Block ${locator.blockOrdinal + 1}`
}

function authorityLabel(reference: ManuscriptReviewAuthorityReference): string {
  if (reference.kind === 'authority_pack') {
    const packId = reference.packId.replace(/^pack:/, '')
    return AUTHORITY_LABELS[packId] ?? packId
  }
  const requirement = reference.reference.normalizedRequirement
  return typeof requirement === 'string' && requirement.trim()
    ? `Institutional requirement: ${requirement}`
    : 'Institutional requirement'
}

function ReviewFindingCard({
  finding,
  stale,
  onNavigate,
}: {
  readonly finding: ManuscriptReviewFinding
  readonly stale: boolean
  readonly onNavigate: (locator: ManuscriptReviewLocator) => void
}) {
  const firstLocation = finding.manuscriptLocators[0]
  return (
    <details className="manuscript-review-finding">
      <summary
        className="manuscript-review-finding-summary"
        onClick={() => {
          if (firstLocation) onNavigate(firstLocation)
        }}
      >
        <span className="manuscript-review-rank">{finding.priorityRank}</span>
        <span className="manuscript-review-statement">{finding.statement}</span>
        {firstLocation && (
          <span className="manuscript-review-location-indicator">
            {locationLabel(firstLocation)}
          </span>
        )}
      </summary>
      <div className="manuscript-review-finding-body">
        <section>
          <h3>Explanation</h3>
          <p>{finding.explanation}</p>
        </section>

        {finding.evidence.length > 0 && (
          <section>
            <h3>Manuscript evidence</h3>
            {finding.evidence.map((item, index) => (
              <blockquote key={`${finding.id}-evidence-${index}`}>
                <p>{item.excerpt}</p>
                <cite>{locationLabel(item.locator)}</cite>
              </blockquote>
            ))}
          </section>
        )}

        {finding.manuscriptLocators.length > 0 && (
          <section>
            <h3>Manuscript locations</h3>
            <div className="manuscript-review-location-list">
              {finding.manuscriptLocators.map((locator, index) => (
                <button
                  key={`${finding.id}-location-${index}`}
                  type="button"
                  className="btn-ghost manuscript-review-location"
                  disabled={stale}
                  onClick={() => onNavigate(locator)}
                >
                  {`Go to ${locationLabel(locator)}`}
                </button>
              ))}
            </div>
          </section>
        )}

        {finding.authorityReferences.length > 0 && (
          <section>
            <h3>Authority</h3>
            <ul className="manuscript-review-authorities">
              {finding.authorityReferences.map((reference, index) => (
                <li key={`${finding.id}-authority-${index}`}>{authorityLabel(reference)}</li>
              ))}
            </ul>
          </section>
        )}
      </div>
    </details>
  )
}

export function ManuscriptReviewPanel({
  editor,
  documentId,
  transport,
  onClose,
}: ManuscriptReviewPanelProps) {
  const { t } = useI18n()
  const [review, setReview] = useState<ReviewState>({ kind: 'idle' })
  const [currentVersion, setCurrentVersion] = useState<number | null>(null)
  const [navigationMessage, setNavigationMessage] = useState<string | null>(null)
  const hasDocument = documentId.trim().length > 0
  const result = review.kind === 'loaded' ? review.result : null
  const stale = result !== null && currentVersion !== result.documentVersion

  useEffect(() => {
    if (!result || !transport) return

    let disposed = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const refreshVersion = async () => {
      try {
        const active = await transport.activeDocument(documentId)
        if (!disposed) setCurrentVersion(active.version)
      } catch {
        if (!disposed) setCurrentVersion(null)
      }
    }
    const scheduleRefresh = () => {
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => void refreshVersion(), 250)
    }

    void refreshVersion()
    editor.on('update', scheduleRefresh)
    return () => {
      disposed = true
      if (timer) clearTimeout(timer)
      editor.off('update', scheduleRefresh)
    }
  }, [documentId, editor, result, transport])

  const runReview = async () => {
    if (!transport || !hasDocument || review.kind === 'running') return
    setReview({ kind: 'running' })
    setNavigationMessage(null)
    try {
      const active = await transport.activeDocument(documentId)
      if (active.availability !== 'available') throw new Error('document unavailable')
      setCurrentVersion(active.version)
      const nextResult = await transport.runManuscriptReview({
        documentId,
        context: MVP_RESEARCH_CONTEXT,
      })
      setCurrentVersion(nextResult.documentVersion)
      setReview({ kind: 'loaded', result: nextResult })
    } catch (error) {
      const code =
        error instanceof Error && 'code' in error && typeof error.code === 'string'
          ? error.code
          : undefined
      const message =
        code === 'review_model_unavailable'
          ? t('researchReviewModelUnavailable')
          : code === 'review_task_timeout'
            ? t('researchReviewTaskTimeout')
            : code === 'review_synthesis_timeout'
              ? t('researchReviewSynthesisTimeout')
              : code === 'review_synthesis_failed'
                ? t('researchReviewSynthesisFailed')
                : code === 'review_task_failed'
                  ? t('researchReviewTaskFailed')
                  : t('researchReviewRunFailed')
      setReview({ kind: 'error', message })
    }
  }

  const navigate = (locator: ManuscriptReviewLocator) => {
    setNavigationMessage(null)
    if (stale || !navigateToManuscriptReviewLocation(editor, locator)) {
      setNavigationMessage(t('researchReviewLocationUnavailable'))
    }
  }

  return (
    <aside
      className="comments-pane manuscript-review-pane"
      aria-label={t('researchReviewResultsTitle')}
    >
      <div className="comments-pane-head">
        <div>
          <strong className="comments-pane-title">{t('researchReviewResultsTitle')}</strong>
          <span className="citation-review-mode">{t('researchReviewReadOnly')}</span>
        </div>
        <button
          type="button"
          className="comments-pane-close"
          aria-label={t('researchReviewClose')}
          onClick={onClose}
        >
          ×
        </button>
      </div>

      <div className="manuscript-review-scroll">
        {!hasDocument && (
          <p className="manuscript-review-state">{t('researchReviewNoActiveDocument')}</p>
        )}
        {hasDocument && review.kind === 'idle' && (
          <section className="manuscript-review-start">
            <p>{t('researchReviewContextMvp')}</p>
            <button
              type="button"
              className="btn-primary"
              disabled={!transport}
              onClick={() => void runReview()}
            >
              {t('researchReviewManuscript')}
            </button>
          </section>
        )}
        {review.kind === 'running' && (
          <p className="manuscript-review-state" role="status" aria-busy="true">
            {t('researchReviewLoading')}
          </p>
        )}
        {review.kind === 'error' && (
          <section className="manuscript-review-start">
            <p className="manuscript-review-error">{review.message}</p>
            <button
              type="button"
              className="btn-primary"
              disabled={!transport}
              onClick={() => void runReview()}
            >
              {t('researchReviewRerun')}
            </button>
          </section>
        )}
        {result && (
          <>
            <section className="manuscript-review-summary">
              <div>
                <strong>{t('researchReviewComplete')}</strong>
                <span>{`${result.synthesizedFindings.length} ${t('researchReviewFindingCount')}`}</span>
              </div>
              <button
                type="button"
                className="btn-ghost"
                disabled={review.kind === 'running'}
                onClick={() => void runReview()}
              >
                {t('researchReviewRerun')}
              </button>
            </section>
            {stale && <p className="manuscript-review-stale">{t('researchReviewStale')}</p>}
            {navigationMessage && <p className="manuscript-review-error">{navigationMessage}</p>}
            {result.synthesizedFindings.length === 0 ? (
              <p className="manuscript-review-state">{t('researchReviewNoFindings')}</p>
            ) : (
              <div className="manuscript-review-findings">
                {result.synthesizedFindings.map((finding) => (
                  <ReviewFindingCard
                    key={finding.id}
                    finding={finding}
                    stale={stale}
                    onNavigate={navigate}
                  />
                ))}
              </div>
            )}
          </>
        )}
      </div>
    </aside>
  )
}
