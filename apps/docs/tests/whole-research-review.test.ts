import { act } from 'react'
import { createElement } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Editor } from '@tiptap/core'
import type {
  CoreTransport,
  ManuscriptResearchReviewClaimItem,
  ManuscriptResearchReviewConsistencyItem,
  ManuscriptResearchReviewRun,
} from '@genoffice/9profs-core'
import type { Run } from '@genoffice/docx-engine'
import { blocksToPmDoc } from '../src/renderer/editor/convert'
import { editorExtensions } from '../src/renderer/editor/extensions'
import { findManuscriptClaimRange } from '../src/renderer/editor/research-review-navigation'
import { WholeResearchReviewPanel } from '../src/renderer/components/WholeResearchReviewPanel'

;(globalThis as Record<string, unknown>).IS_REACT_ACT_ENVIRONMENT = true

const editors = new Set<Editor>()

afterEach(() => {
  for (const editor of editors) editor.destroy()
  editors.clear()
})

function editorFor(text = 'Treatment A may reduce mortality.') {
  const runs: Run[] = [{ text }]
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

function summary(overrides: Partial<NonNullable<ManuscriptResearchReviewRun['summary']>> = {}) {
  return {
    totalInventoryClaims: 1,
    coverageReviewSuggestedCount: 1,
    expectationReviewNeededCount: 1,
    assessmentUnavailableCount: 0,
    claimsWithSupportCount: 1,
    claimsWithContradictionCount: 1,
    claimsWithBlockedVerificationCount: 1,
    claimsWithUnverifiedVerificationCount: 1,
    consistencyAssessedCount: 1,
    consistencyConflictCount: 1,
    consistencyCompatibleCount: 0,
    consistencyQualificationCount: 0,
    consistencyEquivalentCount: 0,
    consistencyNotComparableCount: 0,
    consistencyInsufficientContextCount: 1,
    consistencyAssessmentFailureCount: 0,
    coverageContractVersion: '5D4A',
    coverageScope: 'paragraphs and list items',
    coverageLimitations: ['tables are not included'],
    candidateClaimCount: 2,
    candidateBatchCount: 1,
    candidateExpectedWindowCount: 1,
    candidateProcessedWindowCount: 1,
    candidatePairCount: 1,
    ...overrides,
  }
}

function reviewRun(overrides: Partial<ManuscriptResearchReviewRun> = {}): ManuscriptResearchReviewRun {
  return {
    reviewRunId: 'whole-review-1' as never,
    researchCaseId: 'case-1' as never,
    manuscriptSourceId: 'source-1' as never,
    documentId: 'doc-1',
    documentVersion: 7,
    inputHashAlgorithm: 'sha256',
    inputHash: 'input-hash',
    executionIdentityHashAlgorithm: 'sha256',
    executionIdentityHash: 'execution-hash',
    citationReviewRunId: null,
    claimInventoryRunId: 'inventory-1' as never,
    claimCoverageRunId: 'coverage-1' as never,
    citationExpectationRunId: 'expectation-1' as never,
    crossClaimCandidateRunId: 'candidate-1' as never,
    crossClaimAssessmentRunId: 'assessment-1' as never,
    reviewContractVersion: '5D4A',
    status: 'completed',
    failureStage: null,
    failureCode: null,
    createdAtMs: 1,
    completedAtMs: 2,
    summary: summary(),
    ...overrides,
  }
}

function claim(overrides: Partial<ManuscriptResearchReviewClaimItem> = {}): ManuscriptResearchReviewClaimItem {
  return {
    wholeReviewRunId: 'whole-review-1' as never,
    inventoryItemId: 'inventory-item-1' as never,
    ordinal: 1,
    documentBlockId: 'b7',
    blockOrdinal: 0,
    blockKind: 'paragraph',
    sourceStart: 0,
    sourceEnd: 34,
    sourceExcerpt: 'Treatment A may reduce mortality.',
    claimText: 'Treatment A may reduce mortality.',
    claimReviewKind: 'external_evidence',
    bridgeStatus: 'exact_claim_bridge',
    structuralCitationState: 'no_citation_observed_in_block',
    sameBlockCitationCount: 0,
    exactClaimCitationLinkCount: 0,
    targetCount: 0,
    assessmentStatus: 'assessed',
    expectation: 'external_evidence_expected',
    expectationRationale: 'A clinical outcome claim normally needs external evidence.',
    attentionState: 'review_suggested',
    attentionReasons: ['expected_external_evidence_no_exact_citation_link'],
    supportCount: 1,
    contradictionCount: 1,
    contextualizeCount: 1,
    insufficientCount: 1,
    blockedCount: 1,
    unverifiedCount: 1,
    targets: [],
    ...overrides,
  }
}

function consistency(overrides: Partial<ManuscriptResearchReviewConsistencyItem> = {}): ManuscriptResearchReviewConsistencyItem {
  const left = claim({ inventoryItemId: 'left' as never, sourceStart: 0, sourceEnd: 10, sourceExcerpt: 'Treatment A', claimText: 'Treatment A.' })
  const right = claim({ inventoryItemId: 'right' as never, sourceStart: 11, sourceEnd: 20, sourceExcerpt: 'reduces risk', claimText: 'Treatment A reduces risk.' })
  return {
    wholeReviewRunId: 'whole-review-1' as never,
    assessmentItemId: 'assessment-item-1' as never,
    candidateId: 'candidate-item-1' as never,
    left,
    right,
    assessmentStatus: 'assessed',
    relation: 'conflict',
    dimensions: ['direction', 'quantitative'],
    rationale: 'The claims differ in direction.',
    failureCode: null,
    attentionState: 'review_suggested',
    attentionReasons: ['assessed_internal_conflict'],
    ...overrides,
  }
}

function transportFor(options: {
  run?: ManuscriptResearchReviewRun
  claims?: ManuscriptResearchReviewClaimItem[]
  consistency?: ManuscriptResearchReviewConsistencyItem[]
  activeDocument?: ReturnType<typeof vi.fn>
} = {}) {
  const activeDocument = options.activeDocument ?? vi.fn().mockResolvedValue({
    documentId: 'doc-1',
    version: 7,
    availability: 'available',
  })
  const startManuscriptResearchReview = vi.fn().mockResolvedValue(options.run ?? reviewRun())
  const manuscriptResearchReviewClaims = vi.fn().mockResolvedValue(options.claims ?? [claim()])
  const manuscriptResearchReviewConsistency = vi.fn().mockResolvedValue(options.consistency ?? [consistency()])
  const manuscriptCitationReview = vi.fn().mockResolvedValue({ referenceResolutionRunId: 'resolution-run-1' })
  const confirmManuscriptReferenceCandidate = vi.fn().mockResolvedValue([])
  const transport = {
    researchCases: vi.fn().mockResolvedValue([{ caseId: 'case-1', title: 'Clinical review' }]),
    researchSources: vi.fn().mockResolvedValue([
      { sourceId: 'source-1', researchCaseId: 'case-1', kind: 'manuscript', label: 'Draft', identity: null },
      { sourceId: 'legacy', researchCaseId: 'case-1', kind: 'pdf', label: 'Legacy source', identity: null },
    ]),
    createResearchCase: vi.fn(),
    createResearchSource: vi.fn(),
    activeDocument,
    startManuscriptResearchReview,
    manuscriptResearchReviewClaims,
    manuscriptResearchReviewConsistency,
    manuscriptCitationReview,
    confirmManuscriptReferenceCandidate,
  } as unknown as CoreTransport
  return {
    transport,
    activeDocument,
    startManuscriptResearchReview,
    manuscriptResearchReviewClaims,
    manuscriptResearchReviewConsistency,
    manuscriptCitationReview,
    confirmManuscriptReferenceCandidate,
  }
}

function renderPanel(transport: CoreTransport, editor: Editor) {
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  act(() => {
    root.render(createElement(WholeResearchReviewPanel, {
      editor,
      documentId: 'doc-1',
      transport,
      onClose: vi.fn(),
    }))
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

async function chooseContext(container: HTMLElement) {
  await flush()
  const caseSelect = container.querySelector<HTMLSelectElement>('#whole-research-review-case')
  expect(caseSelect).not.toBeNull()
  await act(async () => {
    caseSelect!.value = 'case-1'
    caseSelect!.dispatchEvent(new Event('change', { bubbles: true }))
  })
  await flush()
  const sourceSelect = container.querySelector<HTMLSelectElement>('#whole-research-review-source')
  expect(sourceSelect).not.toBeNull()
  await act(async () => {
    sourceSelect!.value = 'source-1'
    sourceSelect!.dispatchEvent(new Event('change', { bubbles: true }))
  })
  await flush()
}

describe('WholeResearchReviewPanel', () => {
  it('runs one high-level review with one exact active-document snapshot, including zero citations', async () => {
    const editor = editorFor()
    const harness = transportFor()
    const rendered = renderPanel(harness.transport, editor)
    await chooseContext(rendered.container)

    await act(async () => {
      rendered.container.querySelector<HTMLButtonElement>('.citation-review-start')?.click()
    })
    await flush()
    await flush()

    expect(harness.activeDocument).toHaveBeenCalledWith('doc-1')
    expect(harness.startManuscriptResearchReview).toHaveBeenCalledTimes(1)
    const [, input] = harness.startManuscriptResearchReview.mock.calls[0]
    expect(input.documentId).toBe('doc-1')
    expect(input.documentVersion).toBe(7)
    expect(input.citationReviewObservations.citations).toHaveLength(0)
    expect(input.claimInventoryObservations.wholeManuscriptBlocks).toHaveLength(1)
    expect(harness.manuscriptResearchReviewClaims).toHaveBeenCalledWith('whole-review-1')
    expect(harness.manuscriptResearchReviewConsistency).toHaveBeenCalledWith('whole-review-1')
    expect(rendered.container.textContent).not.toContain('No supported citations')
    rendered.unmount()
  })

  it('renders separate evidence and consistency semantics without aggregate scores', async () => {
    const editor = editorFor()
    const harness = transportFor()
    const rendered = renderPanel(harness.transport, editor)
    await chooseContext(rendered.container)
    await act(async () => rendered.container.querySelector<HTMLButtonElement>('.citation-review-start')?.click())
    await flush()
    await flush()
    await act(async () => rendered.container.querySelector<HTMLButtonElement>('[role="tab"][aria-selected="false"]')?.click())

    const text = rendered.container.textContent ?? ''
    expect(text).toContain('Evidence Coverage')
    expect(text).toContain('Internal Consistency')
    expect(text).toContain('Supporting evidence observed')
    expect(text).toContain('Contradictory evidence observed')
    expect(text).toContain('Internal conflict assessed')
    expect(text).toContain('Quantitative')
    expect(text).not.toMatch(/(Research|Evidence|Consistency) (Quality|Score):?\s*\d+%?/)
    expect(text).not.toContain('Valid manuscript')
    rendered.unmount()
  })

  it('keeps Research context explicit and excludes legacy non-manuscript sources', async () => {
    const editor = editorFor()
    const harness = transportFor()
    const rendered = renderPanel(harness.transport, editor)
    await flush()
    const caseSelect = rendered.container.querySelector<HTMLSelectElement>('#whole-research-review-case')!
    await act(async () => {
      caseSelect.value = 'case-1'
      caseSelect.dispatchEvent(new Event('change', { bubbles: true }))
    })
    await flush()
    const sourceSelect = rendered.container.querySelector<HTMLSelectElement>('#whole-research-review-source')!
    expect([...sourceSelect.options].map((option) => option.value)).toEqual(['', 'source-1'])
    expect((rendered.container.querySelector('.citation-review-start') as HTMLButtonElement).disabled).toBe(true)
    rendered.unmount()
  })

  it('renders canonical evidence and confirms one persisted candidate through Core authority', async () => {
    const candidate = {
      candidateId: 'candidate-1',
      resolutionEntryId: 'entry-1',
      sourceId: 'source-2',
      sourceLabel: 'Paper',
      matchKind: 'exact',
    }
    const citationReviewItem = {
      status: 'reference_requires_confirmation',
      resolutionEntryId: 'entry-1',
      candidates: [candidate],
    }
    const target = {
      coverageTargetId: 'coverage-target-1',
      claimCitationLinkId: 'link-1',
      citationOccurrenceId: 'occurrence-1',
      citationTargetId: 'citation-target-1',
      citationReviewItemId: 'citation-item-1',
      bindingId: null,
      sourceId: null,
      sourceSnapshotId: null,
      extractionId: null,
      verificationRunId: null,
      reviewStatus: 'reference_requires_confirmation',
      failureCode: null,
      verificationStatus: null,
      verificationFailureCode: null,
      relation: 'supports',
      rationale: 'Evidence review pending source confirmation.',
      evidenceCount: 1,
      evidence: [{
        evidenceId: 'evidence-1',
        relation: 'supports',
        sourceSnapshotId: 'snapshot-1',
        extractionId: null,
        locator: { kind: 'pdf', page: 4, end_page: null },
        verbatimExcerpt: 'Treatment A reduced mortality in the cohort.',
      }],
      citationReviewItem,
    } as never
    const harness = transportFor({
      run: reviewRun({ citationReviewRunId: 'citation-review-1' as never }),
      claims: [claim({ targetCount: 1, targets: [target] })],
      consistency: [],
    })
    const rendered = renderPanel(harness.transport, editorFor())
    await chooseContext(rendered.container)
    await act(async () => rendered.container.querySelector<HTMLButtonElement>('.citation-review-start')?.click())
    await flush()
    await flush()

    expect(rendered.container.textContent).toContain('Treatment A reduced mortality in the cohort.')
    expect(rendered.container.textContent).toContain('Page 4')
    const confirm = rendered.container.querySelector<HTMLButtonElement>('.whole-research-review-confirm')
    expect(confirm).not.toBeNull()
    await act(async () => confirm?.click())
    await flush()
    expect(harness.manuscriptCitationReview).toHaveBeenCalledWith('citation-review-1')
    expect(harness.confirmManuscriptReferenceCandidate).toHaveBeenCalledWith(
      'resolution-run-1',
      'entry-1',
      'candidate-1',
    )
    expect(rendered.container.textContent).toContain('Run Research Review again')
    expect(rendered.container.querySelector<HTMLButtonElement>('.whole-research-review-go')?.disabled).toBe(true)
    rendered.unmount()
  })

  it('marks a loaded review stale after a debounced Core version refresh', async () => {
    const activeDocument = vi
      .fn()
      .mockResolvedValueOnce({ documentId: 'doc-1', version: 7, availability: 'available' })
      .mockResolvedValue({ documentId: 'doc-1', version: 8, availability: 'available' })
    const harness = transportFor({ activeDocument })
    const editor = editorFor()
    const rendered = renderPanel(harness.transport, editor)
    await chooseContext(rendered.container)
    await act(async () => rendered.container.querySelector<HTMLButtonElement>('.citation-review-start')?.click())
    await flush()
    await flush()

    ;(editor as unknown as { emit: (event: string, payload: unknown) => void }).emit('update', {
      editor,
      transaction: editor.state.tr,
      transactions: [editor.state.tr],
      appendedTransactions: [],
    })
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 300))
    })
    expect(rendered.container.textContent).toContain('Document changed since this Research Review')
    expect(rendered.container.querySelector<HTMLButtonElement>('.whole-research-review-go')?.disabled).toBe(true)
    rendered.unmount()
  })
})

describe('Whole Research Review navigation', () => {
  it('maps Unicode code-point locators to exact PM positions', () => {
    const text = '研究 🧬 shows treatment.'
    const editor = editorFor(text)
    const excerpt = '研究 🧬 shows'
    const range = findManuscriptClaimRange(editor, {
      documentBlockId: 'b7',
      sourceStart: 0,
      sourceEnd: Array.from(excerpt).length,
      sourceExcerpt: excerpt,
    })
    expect(range).not.toBeNull()
    expect(editor.state.doc.textBetween(range!.from, range!.to, '')).toBe(excerpt)
    expect(range!.to - range!.from).toBe(excerpt.length)
  })

  it('fails closed when block identity or excerpt no longer matches', () => {
    const editor = editorFor('研究 🧬 shows treatment.')
    expect(findManuscriptClaimRange(editor, {
      documentBlockId: 'wrong-block',
      sourceStart: 0,
      sourceEnd: 2,
      sourceExcerpt: '研究',
    })).toBeNull()
    expect(findManuscriptClaimRange(editor, {
      documentBlockId: 'b7',
      sourceStart: 0,
      sourceEnd: 2,
      sourceExcerpt: '猜测',
    })).toBeNull()
  })
})
