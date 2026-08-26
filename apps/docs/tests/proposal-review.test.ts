import { act } from 'react'
import { createElement } from 'react'
import { createRoot } from 'react-dom/client'
import { describe, expect, it, vi } from 'vitest'
import type { CoreTransport, DocumentProposal } from '@genoffice/9profs-core'
import { ProposalReview, summarizeProposalChange } from '../src/renderer/ai/ProposalReview'

function proposal(
  documentId: string,
  status: DocumentProposal['status'],
  freshness: DocumentProposal['freshness'],
): DocumentProposal {
  return {
    proposalId: `${documentId}-${status}-${freshness}`,
    changeSetId: `${documentId}-${status}-${freshness}`,
    documentId,
    authority: 'genoffice-active',
    baseVersion: 3,
    status,
    freshness,
    availability: freshness === 'unavailable' ? 'unavailable' : 'available',
    currentVersion: freshness === 'unavailable' ? null : 4,
    createdAtMs: Date.UTC(2026, 7, 26),
    changes: [
      {
        type: 'docs.commandEnvelope',
        payload: { commands: [{ replaceAllText: { find: 'secret-path', replace: 'safe' } }] },
      },
    ],
    retryable: status === 'failed',
  }
}

function render(element: React.ReactElement) {
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  act(() => root.render(element))
  return {
    container,
    unmount: () => {
      act(() => root.unmount())
      container.remove()
    },
  }
}

describe('proposal review', () => {
  it('summarizes existing command structure without exposing payload details', () => {
    expect(
      summarizeProposalChange({
        type: 'docs.commandEnvelope',
        payload: { commands: [{ replaceAllText: { find: 'secret-path' } }] },
      }),
    ).toEqual(['Replace matching text'])
  })

  it('filters to the active document and presents fresh/stale workflow states', async () => {
    const active = proposal('doc-1', 'proposed', 'fresh')
    const stale = proposal('doc-1', 'proposed', 'stale')
    const other = proposal('doc-2', 'applied', 'fresh')
    const transport = {
      documentProposals: vi.fn().mockResolvedValue([active, stale, other]),
    } as unknown as CoreTransport
    const rendered = render(createElement(ProposalReview, { documentId: 'doc-1', transport }))
    await act(async () => {})

    expect(transport.documentProposals).toHaveBeenCalledWith('doc-1')
    expect(rendered.container.textContent).toContain('Replace matching text')
    expect(rendered.container.textContent).not.toContain('secret-path')
    expect(rendered.container.textContent).not.toContain('doc-2-applied')
    expect(rendered.container.querySelectorAll('.ai-proposal-approve')).toHaveLength(2)
    expect(
      rendered.container.querySelectorAll<HTMLButtonElement>('.ai-proposal-approve')[0].disabled,
    ).toBe(false)
    expect(
      rendered.container.querySelectorAll<HTMLButtonElement>('.ai-proposal-approve')[1].disabled,
    ).toBe(true)
    rendered.unmount()
  })

  it('does not send duplicate approval requests while one is in flight', async () => {
    const active = proposal('doc-1', 'proposed', 'fresh')
    const approveDocumentProposal = vi.fn().mockResolvedValue({ ...active, status: 'applied' })
    const transport = {
      documentProposals: vi.fn().mockResolvedValue([active]),
      approveDocumentProposal,
    } as unknown as CoreTransport
    const rendered = render(createElement(ProposalReview, { documentId: 'doc-1', transport }))
    await act(async () => {})

    const approve = rendered.container.querySelector<HTMLButtonElement>('.ai-proposal-approve')!
    await act(async () => {
      approve.click()
      approve.click()
    })
    expect(approveDocumentProposal).toHaveBeenCalledTimes(1)
    rendered.unmount()
  })
})
