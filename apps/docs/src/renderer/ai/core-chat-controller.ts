import {
  createCoreEventClient,
  type CoreEventClient,
  type CoreEventSubscription,
  type CoreTransport,
  type DocsAgentProfile,
} from '@genoffice/9profs-core'

export type DocsAiExecutionMode = 'undecided' | 'core' | 'legacy'

export interface DocsAiModeSelectionOptions {
  readonly currentMode?: DocsAiExecutionMode
  readonly coreTransport?: CoreTransport | null
  readonly documentId?: string
  readonly profile?: DocsAgentProfile | null
  readonly hasAttachments?: boolean
  readonly hasHistoricChat?: boolean
}

export function chooseDocsAiExecutionMode({
  currentMode = 'undecided',
  coreTransport,
  documentId,
  profile,
  hasAttachments = false,
  hasHistoricChat = false,
}: DocsAiModeSelectionOptions): DocsAiExecutionMode {
  if (currentMode !== 'undecided') return currentMode
  return coreTransport &&
    documentId &&
    profile?.readiness === 'ready' &&
    profile.supportsActiveDocsRuns &&
    !hasAttachments &&
    !hasHistoricChat
    ? 'core'
    : 'legacy'
}

export interface CoreDocsToolActivity {
  readonly name: string
  readonly summary: string
  readonly isError?: boolean
}

const SAFE_CORE_TOOL_NAMES = new Set([
  'document.list_active',
  'document.inspect_active',
  'document.propose_active_changes',
])

function safeCoreToolName(name: string): string {
  return SAFE_CORE_TOOL_NAMES.has(name) ? name : 'document tool'
}

function safeCoreToolSummary(name: string): string {
  switch (name) {
    case 'document.list_active':
      return 'List active documents'
    case 'document.inspect_active':
      return 'Inspect document'
    case 'document.propose_active_changes':
      return 'Propose document changes'
    default:
      return 'Document tool'
  }
}

export interface CoreDocsChatResult {
  readonly text: string
  readonly cancelled: boolean
}

export class CoreDocsChatError extends Error {
  readonly code: string
  readonly fallbackAllowed: boolean

  constructor(code: string, fallbackAllowed: boolean) {
    super(coreErrorMessage(code))
    this.name = 'CoreDocsChatError'
    this.code = code
    this.fallbackAllowed = fallbackAllowed
  }
}

function coreErrorMessage(code: string): string {
  switch (code) {
    case 'conversation_busy':
      return 'Document AI is busy. Try again.'
    case 'conversation_unavailable':
      return 'Document AI conversation is unavailable. Start a new chat.'
    case 'provider_not_configured':
    case 'provider_invalid':
      return 'Document AI provider is unavailable.'
    case 'cancel_failed':
      return 'Document AI could not stop the current run.'
    default:
      return 'Document AI could not complete this request.'
  }
}

export interface CoreDocsChatControllerOptions {
  readonly transport: CoreTransport
  readonly documentId: string
  readonly assistantId: string
  readonly createEventClient?: (url: string) => CoreEventClient
  readonly onDelta?: (delta: string) => void
  readonly onToolStarted?: (activity: CoreDocsToolActivity) => void
  readonly onToolCompleted?: (activity: CoreDocsToolActivity) => void
}

interface PendingRun {
  readonly resolve: (result: CoreDocsChatResult) => void
  readonly reject: (error: CoreDocsChatError) => void
}

/** Core-owned Docs conversation lifecycle. No document mutation or approval lives here. */
export class CoreDocsChatController {
  readonly documentId: string
  readonly assistantId: string

  private readonly transport: CoreTransport
  private readonly events: CoreEventClient
  private readonly options: CoreDocsChatControllerOptions
  private conversationId: string | null = null
  private taskId: string | null = null
  private subscription: CoreEventSubscription | null = null
  private pending: PendingRun | null = null
  private creatingConversation = false
  private cancelRequested = false
  private disposed = false
  private generation = 0

  constructor(options: CoreDocsChatControllerOptions) {
    this.options = options
    this.transport = options.transport
    this.documentId = options.documentId
    this.assistantId = options.assistantId
    this.events = (options.createEventClient ?? ((url) => createCoreEventClient({ url })))(
      options.transport.websocketUrl(),
    )
  }

  get currentConversationId(): string | null {
    return this.conversationId
  }

  get busy(): boolean {
    return this.pending !== null || this.creatingConversation
  }

  async run(input: string): Promise<CoreDocsChatResult> {
    if (!input || this.disposed) {
      return Promise.reject(new CoreDocsChatError('core_unavailable', false))
    }
    if (this.busy) {
      return Promise.reject(new CoreDocsChatError('conversation_busy', false))
    }

    const canFallback = this.conversationId === null
    const generation = this.generation
    this.cancelRequested = false
    if (!this.conversationId) {
      this.creatingConversation = true
      try {
        const conversation = await this.transport.createDocumentAgentConversation({
          assistantId: this.assistantId,
          documentId: this.documentId,
        })
        if (this.disposed || generation !== this.generation) {
          throw new CoreDocsChatError('core_unavailable', false)
        }
        this.conversationId = conversation.conversationId
      } catch (error) {
        this.creatingConversation = false
        if (error instanceof CoreDocsChatError) throw error
        throw new CoreDocsChatError('core_unavailable', canFallback)
      }
      this.creatingConversation = false
    }

    if (generation !== this.generation || this.disposed) {
      throw new CoreDocsChatError('core_unavailable', false)
    }
    const conversationId = this.conversationId
    if (!conversationId) throw new CoreDocsChatError('core_unavailable', false)
    let started
    try {
      started = await this.transport.createDocumentAgentConversationRun(conversationId, {
        input,
      })
    } catch {
      throw new CoreDocsChatError('conversation_unavailable', false)
    }

    return new Promise<CoreDocsChatResult>((resolve, reject) => {
      this.taskId = started.task.task_id
      this.pending = { resolve, reject }
      this.subscription = this.events.subscribeToRun(started.run_id, {
        onOutputDelta: (event) => {
          if (!this.cancelRequested) this.options.onDelta?.(event.payload.details.delta)
        },
        onOutputCompleted: (event) => {
          if (!this.cancelRequested) {
            this.settle({ text: event.payload.details.output, cancelled: false })
          }
        },
        onError: (event) => {
          if (!this.cancelRequested)
            this.fail(new CoreDocsChatError(event.payload.details.code, false))
        },
        onToolStarted: (event) =>
          this.options.onToolStarted?.({
            name: safeCoreToolName(event.payload.details.tool),
            summary: safeCoreToolSummary(event.payload.details.tool),
          }),
        onToolCompleted: (event) =>
          this.options.onToolCompleted?.({
            name: safeCoreToolName(event.payload.details.tool),
            summary: safeCoreToolSummary(event.payload.details.tool),
            isError: event.payload.details.is_error,
          }),
      })
      if (this.cancelRequested) void this.cancel()
    })
  }

  async cancel(): Promise<void> {
    this.cancelRequested = true
    const taskId = this.taskId
    if (!this.pending) return
    if (taskId) {
      try {
        await this.transport.cancelAgentTask(taskId)
      } catch {
        this.fail(new CoreDocsChatError('cancel_failed', false))
        return
      }
    }
    this.settle({ text: '', cancelled: true })
  }

  reset(): void {
    this.generation += 1
    const taskId = this.taskId
    if (taskId) void this.transport.cancelAgentTask(taskId).catch(() => undefined)
    this.subscription?.dispose()
    this.subscription = null
    this.settle({ text: '', cancelled: true })
    this.conversationId = null
    this.taskId = null
    this.cancelRequested = false
    this.creatingConversation = false
  }

  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    this.reset()
    this.events.dispose()
  }

  private settle(result: CoreDocsChatResult): void {
    const pending = this.pending
    if (!pending) return
    this.pending = null
    this.taskId = null
    this.subscription?.dispose()
    this.subscription = null
    pending.resolve(result)
  }

  private fail(error: CoreDocsChatError): void {
    const pending = this.pending
    if (!pending) return
    this.pending = null
    this.taskId = null
    this.subscription?.dispose()
    this.subscription = null
    pending.reject(error)
  }
}
