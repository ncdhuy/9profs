import { describe, expect, it, vi } from 'vitest'
import type {
  CoreAgentEventHandlers,
  CoreEventClient,
  CoreEventSubscription,
  CoreTransport,
  AgentExecutionOutputEvent,
  DocsAgentProfile,
} from '@genoffice/9profs-core'
import {
  CoreDocsChatController,
  chooseDocsAiExecutionMode,
} from '../src/renderer/ai/core-chat-controller'

const profile: DocsAgentProfile = {
  defaultAssistantId: 'document-foundation',
  readiness: 'ready',
  assistantAvailability: 'available',
  backendAvailability: 'available',
  providerReady: true,
  capabilities: [
    'document.list_active',
    'document.inspect_active',
    'document.propose_active_changes',
  ],
  supportsActiveDocsRuns: true,
}

class FakeEvents implements CoreEventClient {
  private readonly handlers = new Map<string, CoreAgentEventHandlers>()

  connect(): void {}

  subscribeToRun(runId: string, handlers: CoreAgentEventHandlers): CoreEventSubscription {
    this.handlers.set(runId, handlers)
    return { dispose: () => this.handlers.delete(runId) }
  }

  emit(runId: string, event: AgentExecutionOutputEvent) {
    const handlers = this.handlers.get(runId)
    if (!handlers) return
    if (event.name === 'agent.outputStarted') handlers.onOutputStarted?.(event)
    if (event.name === 'agent.outputDelta') handlers.onOutputDelta?.(event)
    if (event.name === 'agent.outputCompleted') handlers.onOutputCompleted?.(event)
    if (event.name === 'agent.error') handlers.onError?.(event)
    if (event.name === 'agent.toolStarted') handlers.onToolStarted?.(event)
    if (event.name === 'agent.toolCompleted') handlers.onToolCompleted?.(event)
  }

  dispose(): void {
    this.handlers.clear()
  }
}

function transportFor(events: FakeEvents) {
  let run = 0
  let conversation = 0
  const createConversation = vi.fn(async () => {
    conversation += 1
    return {
      conversationId: `conversation-${conversation}`,
      assistantId: profile.defaultAssistantId,
      documentId: 'doc-1',
      state: 'idle' as const,
      turnCount: 0,
      createdAtMs: 1,
      updatedAtMs: 1,
    }
  })
  const createRun = vi.fn(async () => {
    run += 1
    return {
      run_id: `run-${run}`,
      task: { task_id: `task-${run}` },
    }
  })
  const cancel = vi.fn(async () => ({}) as never)
  return {
    transport: {
      websocketUrl: () => 'ws://core/ws',
      createDocumentAgentConversation: createConversation,
      createDocumentAgentConversationRun: createRun,
      cancelAgentTask: cancel,
    } as unknown as CoreTransport,
    createConversation,
    createRun,
    cancel,
    events,
  }
}

const eventBase = { id: 'event-1', occurred_at_ms: 1, payload: { task_id: 'task-1' } }

describe('Docs AI execution selection', () => {
  it('selects Core only for a fresh ready text chat', () => {
    const base = { coreTransport: {} as CoreTransport, documentId: 'doc-1', profile }
    expect(chooseDocsAiExecutionMode(base)).toBe('core')
    expect(chooseDocsAiExecutionMode({ ...base, hasAttachments: true })).toBe('legacy')
    expect(chooseDocsAiExecutionMode({ ...base, hasHistoricChat: true })).toBe('legacy')
    expect(
      chooseDocsAiExecutionMode({
        ...base,
        profile: { ...profile, readiness: 'provider_not_configured' },
      }),
    ).toBe('legacy')
    expect(chooseDocsAiExecutionMode({ ...base, currentMode: 'core', hasAttachments: true })).toBe(
      'core',
    )
    expect(chooseDocsAiExecutionMode({ ...base, currentMode: 'legacy' })).toBe('legacy')
  })
})

describe('CoreDocsChatController', () => {
  it('creates one conversation, streams turns, and reuses it for follow-ups', async () => {
    const events = new FakeEvents()
    const fake = transportFor(events)
    const deltas: string[] = []
    const tools: string[] = []
    const controller = new CoreDocsChatController({
      transport: fake.transport,
      documentId: 'doc-1',
      assistantId: profile.defaultAssistantId,
      createEventClient: () => events,
      onDelta: (delta) => deltas.push(delta),
      onToolStarted: (tool) => tools.push(`start:${tool.name}`),
      onToolCompleted: (tool) => tools.push(`done:${tool.name}`),
    })

    const first = controller.run('Summarize this document')
    await Promise.resolve()
    await Promise.resolve()
    events.emit('run-1', {
      ...eventBase,
      name: 'agent.toolStarted',
      payload: {
        ...eventBase.payload,
        run_id: 'run-1',
        details: { tool_call_id: 'call-1', tool: 'document.inspect_active' },
      },
    })
    events.emit('run-1', {
      ...eventBase,
      name: 'agent.toolCompleted',
      payload: {
        ...eventBase.payload,
        run_id: 'run-1',
        details: {
          tool_call_id: 'call-1',
          tool: 'document.inspect_active',
          is_error: false,
        },
      },
    })
    events.emit('run-1', {
      ...eventBase,
      name: 'agent.outputDelta',
      payload: { ...eventBase.payload, run_id: 'run-1', details: { delta: 'Summary' } },
    })
    events.emit('run-1', {
      ...eventBase,
      name: 'agent.outputCompleted',
      payload: { ...eventBase.payload, run_id: 'run-1', details: { output: 'Summary' } },
    })
    await expect(first).resolves.toEqual({ text: 'Summary', cancelled: false })
    expect(fake.createConversation).toHaveBeenCalledTimes(1)
    expect(fake.createRun).toHaveBeenCalledWith('conversation-1', {
      input: 'Summarize this document',
    })
    expect(deltas).toEqual(['Summary'])
    expect(tools).toEqual(['start:document.inspect_active', 'done:document.inspect_active'])

    const second = controller.run('Make it shorter')
    await Promise.resolve()
    events.emit('run-2', {
      ...eventBase,
      name: 'agent.outputCompleted',
      payload: { ...eventBase.payload, run_id: 'run-2', details: { output: 'Short summary' } },
    })
    await expect(second).resolves.toEqual({ text: 'Short summary', cancelled: false })
    expect(fake.createConversation).toHaveBeenCalledTimes(1)
    expect(fake.createRun).toHaveBeenLastCalledWith('conversation-1', { input: 'Make it shorter' })
    controller.reset()
    expect(controller.currentConversationId).toBeNull()
    controller.dispose()
  })

  it('cancels a task without abandoning its conversation', async () => {
    const events = new FakeEvents()
    const fake = transportFor(events)
    const controller = new CoreDocsChatController({
      transport: fake.transport,
      documentId: 'doc-1',
      assistantId: profile.defaultAssistantId,
      createEventClient: () => events,
    })
    const pending = controller.run('Draft a title')
    await Promise.resolve()
    await Promise.resolve()
    await controller.cancel()
    await expect(pending).resolves.toEqual({ text: '', cancelled: true })
    expect(fake.cancel).toHaveBeenCalledWith('task-1')
    expect(controller.currentConversationId).toBe('conversation-1')
    controller.dispose()
  })

  it('allows first-turn Core setup failure to fall back without creating a second conversation', async () => {
    const events = new FakeEvents()
    const fake = transportFor(events)
    fake.createConversation.mockRejectedValueOnce(new Error('connection refused'))
    const controller = new CoreDocsChatController({
      transport: fake.transport,
      documentId: 'doc-1',
      assistantId: profile.defaultAssistantId,
      createEventClient: () => events,
    })

    await expect(controller.run('Summarize this document')).rejects.toMatchObject({
      fallbackAllowed: true,
    })
    expect(controller.currentConversationId).toBeNull()
    expect(fake.createRun).not.toHaveBeenCalled()
    controller.dispose()
  })
})
