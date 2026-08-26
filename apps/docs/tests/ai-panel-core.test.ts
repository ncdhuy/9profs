import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, createElement } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { Editor } from '@tiptap/core'
import type {
  AgentExecutionOutputEvent,
  CoreTransport,
  DocsAgentProfile,
} from '@genoffice/9profs-core'
import { editorExtensions } from '../src/renderer/editor/extensions'
import { AiPanel } from '../src/renderer/ai/AiPanel'
import { AI_PROVIDERS, type AiSettings } from '../src/shared/ipc'

const settings: AiSettings = {
  provider: 'anthropic',
  providers: Object.fromEntries(
    AI_PROVIDERS.map((provider) => [provider.id, { apiKey: '', model: provider.defaultModel }]),
  ) as AiSettings['providers'],
}

const profile: DocsAgentProfile = {
  defaultAssistantId: 'document-foundation',
  readiness: 'ready',
  assistantAvailability: 'available',
  backendAvailability: 'available',
  providerReady: true,
  capabilities: [],
  supportsActiveDocsRuns: true,
}

class FakeSocket {
  static current: FakeSocket | null = null
  onmessage: ((event: { data: unknown }) => void) | null = null
  onerror: (() => void) | null = null
  onclose: (() => void) | null = null

  constructor() {
    FakeSocket.current = this
  }

  close(): void {
    this.onclose?.()
  }

  deliver(event: AgentExecutionOutputEvent): void {
    this.onmessage?.({ data: JSON.stringify(event) })
  }
}

function outputEvent(
  name: 'agent.outputCompleted',
  runId: string,
  output: string,
): AgentExecutionOutputEvent
function outputEvent(
  name: 'agent.outputDelta',
  runId: string,
  delta: string,
): AgentExecutionOutputEvent
function outputEvent(
  name: 'agent.outputCompleted' | 'agent.outputDelta',
  runId: string,
  value: string,
): AgentExecutionOutputEvent {
  return {
    id: `${runId}-${name}`,
    name,
    occurred_at_ms: 1,
    payload: {
      run_id: runId,
      task_id: `task-${runId}`,
      details: name === 'agent.outputDelta' ? { delta: value } : { output: value },
    },
  } as AgentExecutionOutputEvent
}

function createEditor(): Editor {
  return new Editor({
    element: document.createElement('div'),
    extensions: editorExtensions,
    content: {
      type: 'doc',
      content: [
        {
          type: 'docParagraph',
          attrs: { docxIndex: 0 },
          content: [{ type: 'text', text: 'Text' }],
        },
      ],
    },
  })
}

function mount(
  editor: Editor,
  coreTransport: CoreTransport,
): { container: HTMLElement; root: Root } {
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  act(() =>
    root.render(
      createElement(AiPanel, {
        editor,
        blocks: [],
        settings,
        open: true,
        documentId: 'doc-1',
        coreTransport,
      }),
    ),
  )
  return { container, root }
}

function typeInto(textarea: HTMLTextAreaElement, value: string): void {
  const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!
  act(() => {
    setter.call(textarea, value)
    textarea.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('AiPanel Core mode', () => {
  let desktop: Record<string, unknown>

  beforeEach(() => {
    vi.stubGlobal('WebSocket', FakeSocket)
    Element.prototype.scrollTo ??= () => {}
    desktop = {
      aiStream: vi.fn(),
      aiStreamCancel: vi.fn(),
      onAiStream: () => () => {},
      aiGskStatus: vi.fn().mockResolvedValue({ loggedIn: true }),
      readAttachmentImage: vi.fn(),
    }
    Object.defineProperty(window, 'desktop', { configurable: true, value: desktop })
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('uses one Core conversation for fresh text-only turns', async () => {
    const editor = createEditor()
    let runNumber = 0
    const createConversation = vi.fn().mockResolvedValue({
      conversationId: 'conversation-1',
      assistantId: profile.defaultAssistantId,
      documentId: 'doc-1',
      state: 'idle',
      turnCount: 0,
      createdAtMs: 1,
      updatedAtMs: 1,
    })
    const createRun = vi.fn().mockImplementation(async () => {
      runNumber += 1
      return { run_id: `run-${runNumber}`, task: { task_id: `task-run-${runNumber}` } }
    })
    const transport = {
      websocketUrl: () => 'ws://core/ws',
      documentAgentProfile: vi.fn().mockResolvedValue(profile),
      createDocumentAgentConversation: createConversation,
      createDocumentAgentConversationRun: createRun,
      documentProposals: vi.fn().mockResolvedValue([]),
      cancelAgentTask: vi.fn(),
    } as unknown as CoreTransport
    const { container, root } = mount(editor, transport)
    await act(async () => {
      await flush()
    })

    const textarea = container.querySelector<HTMLTextAreaElement>('.ai-input-box textarea')!
    const send = () => container.querySelector<HTMLButtonElement>('.ai-send-btn')!.click()
    typeInto(textarea, 'Summarize this document')
    act(send)
    await act(async () => {
      await flush()
    })
    expect(container.querySelector('[data-execution-mode="core"]')).not.toBeNull()
    expect(createConversation).toHaveBeenCalledTimes(1)
    expect(createRun).toHaveBeenCalledWith('conversation-1', { input: 'Summarize this document' })
    expect(desktop.aiStream).not.toHaveBeenCalled()

    FakeSocket.current!.deliver(outputEvent('agent.outputDelta', 'run-1', 'Summary'))
    FakeSocket.current!.deliver(outputEvent('agent.outputCompleted', 'run-1', 'Summary'))
    await act(async () => {
      await flush()
    })
    expect(container.textContent).toContain('Summary')

    typeInto(textarea, 'Make it shorter')
    act(send)
    await act(async () => {
      await flush()
    })
    FakeSocket.current!.deliver(outputEvent('agent.outputCompleted', 'run-2', 'Short summary'))
    await act(async () => {
      await flush()
    })
    expect(container.textContent).toContain('Short summary')
    expect(createConversation).toHaveBeenCalledTimes(1)
    expect(createRun).toHaveBeenLastCalledWith('conversation-1', { input: 'Make it shorter' })

    act(() => root.unmount())
    editor.destroy()
    container.remove()
  })
})
