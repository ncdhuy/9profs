import { act, createElement } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Editor } from '@tiptap/core'
import type { CoreTransport, ManuscriptReviewResult } from '@genoffice/9profs-core'
import type { Run } from '@genoffice/docx-engine'
import { blocksToPmDoc } from '../src/renderer/editor/convert'
import { editorExtensions } from '../src/renderer/editor/extensions'
import { ManuscriptReviewPanel } from '../src/renderer/components/ManuscriptReviewPanel'

;(globalThis as Record<string, unknown>).IS_REACT_ACT_ENVIRONMENT = true

const editors = new Set<Editor>()

afterEach(() => {
  for (const editor of editors) editor.destroy()
  editors.clear()
})

function editorFor(text = 'Treatment A may reduce mortality.') {
  const editor = new Editor({
    element: document.createElement('div'),
    extensions: editorExtensions,
    content: blocksToPmDoc([
      {
        id: 'b7',
        type: 'paragraph',
        docxIndex: 7,
        originalXml: null,
        runs: [{ text } as Run],
      } as never,
    ]) as never,
  })
  editors.add(editor)
  return editor
}

function locator(blockId = 'b7', blockOrdinal = 0) {
  return {
    documentId: 'doc-1',
    version: 7,
    blockId,
    blockOrdinal,
    ...(blockId === 'b7' ? { docxIndex: 7 } : {}),
    sectionId: 'section:introduction',
  }
}

function result(overrides: Partial<ManuscriptReviewResult> = {}): ManuscriptReviewResult {
  return {
    documentId: 'doc-1',
    documentVersion: 7,
    synthesizedFindings: [
      {
        id: 'finding-1',
        sourceFindingIds: ['source-1'],
        statement: 'The outcome definition is not sufficiently precise.',
        explanation: 'The manuscript uses two different outcome windows.',
        manuscriptLocators: [locator()],
        evidence: [{ locator: locator(), excerpt: 'Outcome was measured at two windows.' }],
        authorityReferences: [
          {
            kind: 'authority_pack',
            packId: 'pack:research.core',
            version: '1',
            source: {},
            contentPaths: [],
          },
        ],
        priorityRank: 1,
      },
    ],
    summary: {
      taskCount: 1,
      rawFindingCount: 1,
      rejectedFindingCount: 0,
      consolidatedFindingCount: 1,
    },
    ...overrides,
  }
}

function transportFor(nextResult: ManuscriptReviewResult | Promise<ManuscriptReviewResult>) {
  return {
    activeDocument: vi.fn().mockResolvedValue({
      documentId: 'doc-1',
      documentType: 'docx',
      authority: 'genoffice.docs',
      version: 7,
      capabilities: [],
      availability: 'available',
    }),
    runManuscriptReview: vi.fn().mockImplementation(() => Promise.resolve(nextResult)),
  } as unknown as CoreTransport
}

function renderPanel(transport: CoreTransport, editor: Editor) {
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root: Root = createRoot(container)
  act(() => {
    root.render(
      createElement(ManuscriptReviewPanel, {
        editor,
        documentId: 'doc-1',
        transport,
        onClose: vi.fn(),
      }),
    )
  })
  return {
    container,
    root,
    cleanup: () => {
      act(() => root.unmount())
      container.remove()
    },
  }
}

async function clickReview(container: HTMLElement) {
  await act(async () => {
    container.querySelector<HTMLButtonElement>('.manuscript-review-start button')!.click()
  })
}

describe('ManuscriptReviewPanel', () => {
  it('explicitly starts the active-document review and renders synthesis order/count', async () => {
    const editor = editorFor()
    const first = result()
    const second = {
      ...first.synthesizedFindings[0],
      id: 'finding-2',
      statement: 'A second issue.',
    }
    const transport = transportFor(
      result({ synthesizedFindings: [first.synthesizedFindings[0], second] }),
    )
    const rendered = renderPanel(transport, editor)

    await clickReview(rendered.container)

    expect(transport.runManuscriptReview).toHaveBeenCalledWith({
      documentId: 'doc-1',
      context: {
        language: 'vi',
        researchFamilies: ['MED'],
        artifactType: 'master_thesis',
        academicLevel: 'master',
        studyDesigns: [],
        reportingGuidelines: [],
        organization: 'hiu',
      },
    })
    expect(rendered.container.textContent).toContain('Review complete')
    expect(rendered.container.textContent).toContain('2 findings')
    expect(
      rendered.container.textContent!.indexOf(first.synthesizedFindings[0].statement),
    ).toBeLessThan(rendered.container.textContent!.indexOf('A second issue.'))
    expect(rendered.container.textContent).not.toContain('HIGH_VALUE')
    rendered.cleanup()
  })

  it('renders a running state and prevents duplicate runs', async () => {
    const editor = editorFor()
    let resolve: ((value: ManuscriptReviewResult) => void) | undefined
    const pending = new Promise<ManuscriptReviewResult>((nextResolve) => {
      resolve = nextResolve
    })
    const transport = transportFor(pending)
    const rendered = renderPanel(transport, editor)

    await act(async () => {
      rendered.container
        .querySelector<HTMLButtonElement>('.manuscript-review-start button')!
        .click()
      await Promise.resolve()
    })
    expect(rendered.container.textContent).toContain('Reviewing manuscript')
    expect(transport.runManuscriptReview).toHaveBeenCalledTimes(1)

    resolve!(result())
    await act(async () => {
      await pending
    })
    rendered.cleanup()
  })

  it('renders a safe model-provider failure message', async () => {
    const editor = editorFor()
    const error = Object.assign(new Error('provider unavailable'), {
      code: 'review_model_unavailable',
    })
    const transport = transportFor(Promise.reject(error))
    const rendered = renderPanel(transport, editor)

    await clickReview(rendered.container)

    expect(rendered.container.textContent).toContain(
      'Research Review could not reach the model provider. Try again later.',
    )
    expect(rendered.container.textContent).not.toContain('provider unavailable')
    rendered.cleanup()
  })

  it('expands explanation, evidence, authority, and multiple locations', async () => {
    const editor = editorFor()
    const finding = result().synthesizedFindings[0]
    const transport = transportFor(
      result({
        synthesizedFindings: [
          {
            ...finding,
            manuscriptLocators: [locator(), locator('b8', 1)],
          },
        ],
      }),
    )
    const rendered = renderPanel(transport, editor)
    await clickReview(rendered.container)

    act(() =>
      rendered.container
        .querySelector('summary')!
        .dispatchEvent(new MouseEvent('click', { bubbles: true })),
    )
    expect(rendered.container.textContent).toContain(
      'The manuscript uses two different outcome windows.',
    )
    expect(rendered.container.textContent).toContain('Outcome was measured at two windows.')
    expect(rendered.container.textContent).toContain('Research principles')
    expect(rendered.container.querySelectorAll('.manuscript-review-location')).toHaveLength(2)
    rendered.cleanup()
  })

  it('navigates to a valid block without changing document content', async () => {
    const editor = editorFor()
    const dispatch = vi.spyOn(editor.view, 'dispatch')
    const transport = transportFor(result())
    const rendered = renderPanel(transport, editor)
    await clickReview(rendered.container)

    act(() =>
      rendered.container
        .querySelector('summary')!
        .dispatchEvent(new MouseEvent('click', { bubbles: true })),
    )

    expect(dispatch).toHaveBeenCalled()
    expect(dispatch.mock.calls.every(([transaction]) => transaction.docChanged === false)).toBe(
      true,
    )
    rendered.cleanup()
  })

  it('marks changed results stale and disables precise navigation', async () => {
    const editor = editorFor()
    const transport = transportFor(result()) as unknown as {
      activeDocument: ReturnType<typeof vi.fn>
      runManuscriptReview: ReturnType<typeof vi.fn>
    }
    const rendered = renderPanel(transport as unknown as CoreTransport, editor)
    await clickReview(rendered.container)
    transport.activeDocument.mockResolvedValue({ version: 8, availability: 'available' })

    act(() =>
      editor.emit('update', {
        editor,
        transaction: editor.state.tr,
        appendedTransactions: [],
      }),
    )
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 300))
    })

    expect(rendered.container.textContent).toContain('Document changed since this Research Review')
    expect(
      rendered.container.querySelector<HTMLButtonElement>('.manuscript-review-location')!.disabled,
    ).toBe(true)
    rendered.cleanup()
  })

  it('fails safely when a locator is unavailable and reports execution failure', async () => {
    const editor = editorFor()
    const transport = transportFor(
      result({
        synthesizedFindings: [
          { ...result().synthesizedFindings[0], manuscriptLocators: [locator('b404', 4)] },
        ],
      }),
    )
    const rendered = renderPanel(transport, editor)
    await clickReview(rendered.container)
    act(() =>
      rendered.container.querySelector<HTMLButtonElement>('.manuscript-review-location')!.click(),
    )
    expect(rendered.container.textContent).toContain('This finding location is unavailable')
    rendered.cleanup()

    const failing = transportFor(Promise.reject(new Error('provider failure')))
    const failed = renderPanel(failing, editor)
    await clickReview(failed.container)
    expect(failed.container.textContent).toContain(
      'Research Review could not complete. Check Research Core and try again.',
    )
    expect(failed.container.textContent).not.toContain('provider failure')
    failed.cleanup()
  })
})
