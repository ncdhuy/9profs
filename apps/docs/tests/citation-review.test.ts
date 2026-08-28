import { act } from 'react'
import { createElement } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Editor } from '@tiptap/core'
import type { CitationReviewItem, CitationReviewRun, CoreTransport } from '@genoffice/9profs-core'
import type { DocxCitation, Run } from '@genoffice/docx-engine'
import {
  blocksToPmDoc,
  extractDocxCitationsFromPmDoc,
  type PmNode,
} from '../src/renderer/editor/convert'
import { editorExtensions } from '../src/renderer/editor/extensions'
import { findCitationNodePosition } from '../src/renderer/editor/citation-review-navigation'
import {
  CitationReviewPanel,
  citationReviewAllowsCandidateConfirmation,
  citationReviewNeedsAttention,
} from '../src/renderer/components/CitationReviewPanel'

;(globalThis as Record<string, unknown>).IS_REACT_ACT_ENVIRONMENT = true

const editors = new Set<Editor>()

afterEach(() => {
  for (const editor of editors) editor.destroy()
  editors.clear()
})

const citation: DocxCitation = {
  format: 'Zotero',
  renderedText: '[12,13]',
  instruction: ' ADDIN ZOTERO_ITEM CSL_CITATION {"citationItems":[{"id":12},{"id":13}]}',
  targets: [
    { ordinal: 1, referenceKey: '12', itemId: '12' },
    { ordinal: 2, referenceKey: '13', itemId: '13' },
  ],
  originalXml: '<w:r><w:t>[12,13]</w:t></w:r>',
}

function editorForCitation(prefix = 'Drug A works '): Editor {
  const runs: Run[] = [
    { text: prefix },
    { text: citation.renderedText, citation },
    { text: ' in adults.' },
  ]
  const editor = new Editor({
    element: document.createElement('div'),
    extensions: editorExtensions,
    content: blocksToPmDoc([
      {
        id: 'b7',
        type: 'paragraph',
        docxIndex: 7,
        originalXml: null,
        runs,
      } as never,
    ]) as never,
  })
  editors.add(editor)
  return editor
}

function reviewRun(): CitationReviewRun {
  return {
    reviewRunId: 'review-1' as never,
    researchCaseId: 'case-1' as never,
    manuscriptSourceId: 'source-1' as never,
    documentId: 'doc-1',
    documentVersion: 7,
    citationSyncRunId: 'sync-1',
    referenceCatalogRunId: 'catalog-1',
    referenceResolutionRunId: 'resolution-1',
    claimExtractionRunId: 'claims-1',
    status: 'completed',
    failureStage: null,
    failureCode: null,
    createdAtMs: 1,
    completedAtMs: 2,
  }
}

function reviewItem(
  referenceKey: string,
  itemId: string,
  overrides: Partial<CitationReviewItem> = {},
): CitationReviewItem {
  return {
    itemId,
    reviewRunId: 'review-1' as never,
    ordinal: 0,
    claimId: `claim-${referenceKey}` as never,
    claimCitationLinkId: `link-${referenceKey}`,
    citationOccurrenceId: 'occurrence-1',
    citationTargetId: `target-${referenceKey}`,
    referenceEntryId: `entry-${referenceKey}`,
    resolutionEntryId: 'resolution-entry-1',
    resolutionOutcome: 'candidate_requires_confirmation',
    documentBlockId: 'b7',
    start: 13,
    end: 20,
    renderedText: '[12,13]',
    referenceKey,
    citedLocator: null,
    claimText: 'Drug A works.',
    sourceExcerpt: 'Drug A works in adults.',
    bindingId: null,
    bindingMethod: null,
    sourceId: null,
    sourceSnapshotId: null,
    extractionId: null,
    status: 'reference_requires_confirmation',
    failureCode: null,
    candidates: [
      {
        candidateId: `candidate-${referenceKey}`,
        resolutionEntryId: 'resolution-entry-1',
        ordinal: 1,
        sourceId: 'source-2' as never,
        sourceLabel: 'Reference PDF',
        sourceSnapshotId: 'snapshot-1' as never,
        extractionId: 'extraction-1' as never,
        matchKind: 'reference_key_source_label',
        automaticBindingPermitted: false,
      },
    ],
    verification: null,
    evidence: [],
    ...overrides,
  }
}

function completedVerification(): NonNullable<CitationReviewItem['verification']> {
  return {
    verificationRunId: 'verification-1' as never,
    status: 'completed',
    failureCode: null,
    relation: 'supports',
    rationale: 'The evidence supports the claim.',
    assessorProvider: 'core',
    assessorVersion: '1',
    assessorModelId: 'model-1',
    completedAtMs: 3,
  }
}

function renderPanel(transport: CoreTransport, editor: Editor) {
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  act(() => {
    root.render(
      createElement(CitationReviewPanel, {
        editor,
        documentId: 'doc-1',
        transport,
        onClose: vi.fn(),
      }),
    )
  })
  return {
    container,
    unmount: () => {
      act(() => root.unmount())
      container.remove()
    },
  }
}

async function flush() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
}

describe('citation review navigation', () => {
  it('resolves Unicode-offset grouped targets to the citation atom and fails closed', () => {
    const editor = editorForCitation('研究 🧬 cho thấy ')
    const descriptor = extractDocxCitationsFromPmDoc(editor.getJSON() as PmNode)[0]
    const first = reviewItem('12', 'item-12', {
      documentBlockId: descriptor.blockId,
      start: descriptor.start,
      end: descriptor.end,
      renderedText: descriptor.renderedText,
    })
    const second = reviewItem('13', 'item-13', {
      documentBlockId: descriptor.blockId,
      start: descriptor.start,
      end: descriptor.end,
      renderedText: descriptor.renderedText,
    })
    const firstPosition = findCitationNodePosition(editor, first)
    const secondPosition = findCitationNodePosition(editor, second)
    let actualPosition = -1
    editor.state.doc.descendants((node, pos) => {
      if (node.type.name === 'docxCitation') actualPosition = pos
    })
    expect(firstPosition).toBe(actualPosition)
    expect(firstPosition).toBeGreaterThan(0)
    expect(firstPosition).not.toBe(descriptor.start)
    expect(secondPosition).toBe(firstPosition)
    expect(findCitationNodePosition(editor, { ...first, referenceKey: 'missing' })).toBeNull()
  })
})

describe('citation review panel', () => {
  function transportFor(items: CitationReviewItem[], activeVersion = 7) {
    const activeDocument = vi.fn().mockImplementation(async () => ({
      documentId: 'doc-1',
      documentType: 'docx',
      authority: 'docs',
      version: activeVersion,
      capabilities: [],
      availability: 'available',
    }))
    const confirmManuscriptReferenceCandidate = vi.fn().mockResolvedValue([])
    const transport = {
      researchCases: vi
        .fn()
        .mockResolvedValue([{ caseId: 'case-1', title: 'Trial', createdAtMs: 1, updatedAtMs: 1 }]),
      researchSources: vi.fn().mockResolvedValue([
        {
          sourceId: 'source-1',
          researchCaseId: 'case-1',
          kind: 'manuscript',
          label: 'Draft',
          createdAtMs: 1,
        },
      ]),
      activeDocument,
      startManuscriptCitationReview: vi.fn().mockResolvedValue(reviewRun()),
      manuscriptCitationReviewItems: vi.fn().mockResolvedValue(items),
      confirmManuscriptReferenceCandidate,
    } as unknown as CoreTransport
    return { transport, activeDocument, confirmManuscriptReferenceCandidate }
  }

  async function loadPanel(items: CitationReviewItem[]) {
    const editor = editorForCitation()
    const { transport, activeDocument, confirmManuscriptReferenceCandidate } = transportFor(items)
    const rendered = renderPanel(transport, editor)
    await flush()
    await flush()
    await act(async () => {
      rendered.container.querySelector<HTMLButtonElement>('.citation-review-start')?.click()
    })
    await flush()
    return { editor, rendered, activeDocument, confirmManuscriptReferenceCandidate }
  }

  it('shows separate target cards, one confirmation control per shared resolution entry, and no editor mutation', async () => {
    const editor = editorForCitation()
    const before = editor.getJSON()
    const { transport, confirmManuscriptReferenceCandidate } = transportFor([
      reviewItem('12', 'item-12'),
      reviewItem('13', 'item-13'),
    ])
    const rendered = renderPanel(transport, editor)
    await flush()
    await flush()
    await act(async () => {
      rendered.container.querySelector<HTMLButtonElement>('.citation-review-start')?.click()
    })
    await flush()
    expect(transport.startManuscriptCitationReview).toHaveBeenCalledTimes(1)
    expect(rendered.container.querySelectorAll('.citation-review-card')).toHaveLength(2)
    expect(rendered.container.querySelectorAll('.citation-review-confirm')).toHaveLength(1)
    expect(rendered.container.querySelector('.citation-review-go')).not.toBeNull()
    await act(async () => {
      rendered.container.querySelector<HTMLButtonElement>('.citation-review-confirm')?.click()
    })
    await flush()
    expect(confirmManuscriptReferenceCandidate).toHaveBeenCalledWith(
      'resolution-1',
      'resolution-entry-1',
      'candidate-12',
    )
    expect(
      rendered.container.querySelector<HTMLButtonElement>('.citation-review-go')?.disabled,
    ).toBe(true)
    expect(editor.getJSON()).toEqual(before)
    rendered.unmount()
  })

  it('marks navigation and confirmation stale after the active document version changes', async () => {
    const editor = editorForCitation()
    let activeVersion = 7
    const { transport, activeDocument } = transportFor([reviewItem('12', 'item-12')])
    activeDocument.mockImplementation(async () => ({
      documentId: 'doc-1',
      documentType: 'docx',
      authority: 'docs',
      version: activeVersion,
      capabilities: [],
      availability: 'available',
    }))
    const rendered = renderPanel(transport, editor)
    await flush()
    await flush()
    await act(async () => {
      rendered.container.querySelector<HTMLButtonElement>('.citation-review-start')?.click()
    })
    await flush()
    activeVersion = 8
    await act(async () => {
      editor.commands.insertContentAt(2, 'x')
    })
    await flush()
    expect(
      rendered.container.querySelector<HTMLButtonElement>('.citation-review-go')?.disabled,
    ).toBe(true)
    expect(
      rendered.container.querySelector<HTMLButtonElement>('.citation-review-confirm')?.disabled,
    ).toBe(true)
    expect(activeDocument).toHaveBeenCalled()
    rendered.unmount()
  })

  it('keeps historical candidates visible without actions for Human-bound effective states', async () => {
    const fixtures: Array<Partial<CitationReviewItem>> = [
      {
        status: 'verification_completed',
        bindingId: 'binding-human',
        bindingMethod: 'human',
        verification: completedVerification(),
      },
      {
        status: 'source_matched_not_verification_ready',
        bindingId: 'binding-human',
        bindingMethod: 'human',
      },
    ]
    for (const overrides of fixtures) {
      const { rendered } = await loadPanel([reviewItem('12', 'item-12', overrides)])
      expect(rendered.container.querySelectorAll('.citation-review-details')).toHaveLength(2)
      expect(rendered.container.querySelectorAll('.citation-review-confirm')).toHaveLength(0)
      rendered.unmount()
    }
    expect(
      citationReviewAllowsCandidateConfirmation(
        reviewItem('12', 'item-12', { status: 'verification_completed' }),
      ),
    ).toBe(false)
    for (const status of [
      'source_matched_not_verification_ready',
      'ready_for_verification',
      'verification_running',
      'verification_completed',
      'verification_failed',
      'binding_conflict',
      'unresolved_reference',
      'resolution_failed',
    ] as const) {
      expect(
        citationReviewAllowsCandidateConfirmation(reviewItem('12', 'item-12', { status })),
      ).toBe(false)
    }
  })

  it('allows ambiguous references to choose persisted candidates once per grouped entry', async () => {
    const base = reviewItem('12', 'item-12')
    const candidates = [
      ...base.candidates,
      { ...base.candidates[0], candidateId: 'candidate-13', ordinal: 2 },
    ]
    const first = reviewItem('12', 'item-12', {
      status: 'ambiguous_reference',
      candidates,
    })
    const second = reviewItem('13', 'item-13', {
      status: 'ambiguous_reference',
      candidates,
    })
    const { rendered } = await loadPanel([first, second])
    expect(rendered.container.querySelectorAll('.citation-review-card')).toHaveLength(2)
    expect(rendered.container.querySelectorAll('.citation-review-confirm')).toHaveLength(2)
    rendered.unmount()
  })

  it('does not resurrect confirmation through All, Needs attention, or relation filters', async () => {
    const { rendered } = await loadPanel([
      reviewItem('12', 'item-12', {
        status: 'verification_completed',
        bindingId: 'binding-human',
        bindingMethod: 'human',
        verification: completedVerification(),
      }),
    ])
    const filter = rendered.container.querySelector<HTMLSelectElement>('.citation-review-filter')
    expect(filter).not.toBeNull()
    for (const value of [
      'all',
      'needs',
      'supports',
      'contradicts',
      'contextualizes',
      'insufficient',
    ]) {
      await act(async () => {
        filter!.value = value
        filter!.dispatchEvent(new Event('change', { bubbles: true }))
      })
      expect(rendered.container.querySelectorAll('.citation-review-confirm')).toHaveLength(0)
    }
    rendered.unmount()
  })

  it('exposes attention filtering and keeps Core unavailable explicit', async () => {
    expect(citationReviewNeedsAttention({ status: 'reference_requires_confirmation' })).toBe(true)
    expect(citationReviewNeedsAttention({ status: 'verification_completed' })).toBe(false)
    const editor = editorForCitation()
    const rendered = renderPanel(null as unknown as CoreTransport, editor)
    await flush()
    expect(rendered.container.textContent).toContain('Research Core')
    rendered.unmount()
  })
})
