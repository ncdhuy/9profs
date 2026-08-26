import { useCallback, useEffect, useRef, useState } from 'react'
import type { CoreTransport, DocumentProposal, DocumentProposalChange } from '@genoffice/9profs-core'

interface ProposalReviewProps {
  readonly documentId?: string
  readonly transport?: CoreTransport | null
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value)
}

const COMMAND_LABELS: Record<string, string> = {
  replaceAllText: 'Replace matching text',
  insertText: 'Insert text',
  deleteText: 'Delete text',
  deleteBlocks: 'Delete blocks',
  moveBlocks: 'Move blocks',
  applyParagraphStyle: 'Apply paragraph style',
  setHeadingLevel: 'Set heading level',
  insertToc: 'Insert table of contents',
}

function humanizeCommandName(name: string): string {
  return COMMAND_LABELS[name] ?? name.replace(/([a-z])([A-Z])/g, '$1 $2').toLowerCase()
}

export function summarizeProposalChange(change: DocumentProposalChange): string[] {
  if (!isRecord(change.payload) || !Array.isArray(change.payload.commands)) {
    return ['Review document change']
  }
  return change.payload.commands.map((command) => {
    if (!isRecord(command)) return 'Run document command'
    const name = Object.keys(command)[0]
    return name ? humanizeCommandName(name) : 'Run document command'
  })
}

function summarizeProposal(proposal: DocumentProposal): string[] {
  const summaries = proposal.changes.flatMap(summarizeProposalChange)
  if (summaries.length > 0) return summaries
  const summary = proposal.summary
    ?.replace(/\b[A-Za-z]:[\\/][^\s]+/g, '[details hidden]')
    .replace(/(^|\s)\/[^\s]+/g, '$1[details hidden]')
    .slice(0, 240)
  return [summary || 'Review document change']
}

function statusLabel(proposal: DocumentProposal): string {
  if (proposal.status === 'proposed' && proposal.freshness !== 'fresh') {
    return proposal.freshness === 'stale' ? 'Stale' : 'Unavailable'
  }
  if (proposal.status === 'proposed') return 'Proposed · Fresh'
  return proposal.status.charAt(0).toUpperCase() + proposal.status.slice(1)
}

export function ProposalReview({ documentId, transport }: ProposalReviewProps) {
  const [proposals, setProposals] = useState<DocumentProposal[]>([])
  const [busy, setBusy] = useState<Record<string, boolean>>({})
  const [error, setError] = useState<string | null>(null)
  const inFlight = useRef(new Set<string>())

  const refresh = useCallback(async () => {
    if (!transport || !documentId) {
      setProposals([])
      return
    }
    try {
      const next = await transport.documentProposals(documentId)
      setProposals(next.filter((proposal) => proposal.documentId === documentId))
      setError(null)
    } catch {
      setError('Proposal review is temporarily unavailable.')
    }
  }, [documentId, transport])

  useEffect(() => {
    let disposed = false
    void refresh()
    const timer = window.setInterval(() => {
      if (!disposed) void refresh()
    }, 2500)
    return () => {
      disposed = true
      window.clearInterval(timer)
    }
  }, [refresh])

  const decide = async (proposalId: string, action: 'approve' | 'reject' | 'retry') => {
    if (!transport || inFlight.current.has(proposalId)) return
    inFlight.current.add(proposalId)
    setBusy((current) => ({ ...current, [proposalId]: true }))
    try {
      if (action === 'approve') await transport.approveDocumentProposal(proposalId)
      if (action === 'reject') await transport.rejectDocumentProposal(proposalId)
      if (action === 'retry') await transport.retryDocumentProposal(proposalId)
      await refresh()
    } catch {
      setError('Proposal review action could not be completed.')
    } finally {
      inFlight.current.delete(proposalId)
      setBusy((current) => ({ ...current, [proposalId]: false }))
    }
  }

  if (!transport || !documentId || proposals.length === 0) return null

  return (
    <section className="ai-proposal-review" aria-label="Document change proposals">
      <div className="ai-proposal-review-header">
        <strong>Review proposed changes</strong>
        <button type="button" className="ai-proposal-refresh" onClick={() => void refresh()}>
          Refresh
        </button>
      </div>
      {error && <div className="ai-proposal-error">{error}</div>}
      {proposals.map((proposal) => {
        const summaries = summarizeProposal(proposal)
        const isPending = proposal.status === 'proposed'
        const isFresh = proposal.freshness === 'fresh'
        const isBusy = busy[proposal.proposalId] === true
        return (
          <article className="ai-proposal-card" key={proposal.proposalId}>
            <div className="ai-proposal-card-topline">
              <span className="ai-proposal-status">{statusLabel(proposal)}</span>
              <span className="ai-proposal-version">
                Base v{proposal.baseVersion}
                {proposal.currentVersion === null ? '' : ` · Current v${proposal.currentVersion}`}
              </span>
            </div>
            <div className="ai-proposal-meta">
              Created {new Date(proposal.createdAtMs).toLocaleString()} · {proposal.changes.length}{' '}
              {proposal.changes.length === 1 ? 'change' : 'changes'} · {summaries.length}{' '}
              {summaries.length === 1 ? 'command' : 'commands'}
            </div>
            <ul className="ai-proposal-summary">
              {summaries.slice(0, 4).map((summary, index) => (
                <li key={`${summary}-${index}`}>{summary}</li>
              ))}
              {summaries.length > 4 && <li>More document changes</li>}
            </ul>
            <div className="ai-proposal-actions">
              {isPending && (
                <>
                  <button
                    type="button"
                    className="ai-proposal-approve"
                    disabled={isBusy || !isFresh}
                    onClick={() => void decide(proposal.proposalId, 'approve')}
                  >
                    Approve
                  </button>
                  <button
                    type="button"
                    className="ai-proposal-reject"
                    disabled={isBusy}
                    onClick={() => void decide(proposal.proposalId, 'reject')}
                  >
                    Reject
                  </button>
                </>
              )}
              {proposal.status === 'failed' && proposal.retryable && (
                <button
                  type="button"
                  className="ai-proposal-retry"
                  disabled={isBusy}
                  onClick={() => void decide(proposal.proposalId, 'retry')}
                >
                  Retry
                </button>
              )}
              {proposal.status === 'failed' && !proposal.retryable && (
                <span className="ai-proposal-working">Application failed</span>
              )}
              {isBusy && <span className="ai-proposal-working">Working…</span>}
            </div>
          </article>
        )
      })}
    </section>
  )
}
