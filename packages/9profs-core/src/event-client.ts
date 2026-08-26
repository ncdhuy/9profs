import type { AgentExecutionOutputEvent, AgentRunId } from './types'

export interface CoreEventClientOptions {
  readonly url: string
  readonly createWebSocket?: (url: string) => WebSocket
  readonly reconnect?: boolean
  readonly reconnectDelayMs?: number
}

export interface CoreAgentEventHandlers {
  readonly onOutputStarted?: (
    event: Extract<AgentExecutionOutputEvent, { readonly name: 'agent.outputStarted' }>,
  ) => void
  readonly onOutputDelta?: (
    event: Extract<AgentExecutionOutputEvent, { readonly name: 'agent.outputDelta' }>,
  ) => void
  readonly onOutputCompleted?: (
    event: Extract<AgentExecutionOutputEvent, { readonly name: 'agent.outputCompleted' }>,
  ) => void
  readonly onError?: (
    event: Extract<AgentExecutionOutputEvent, { readonly name: 'agent.error' }>,
  ) => void
}

export interface CoreEventSubscription {
  dispose(): void
}

export interface CoreEventClient {
  connect(): void
  subscribeToRun(runId: AgentRunId, handlers: CoreAgentEventHandlers): CoreEventSubscription
  dispose(): void
}

type Socket = WebSocket

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function parseAgentEvent(raw: unknown): AgentExecutionOutputEvent | undefined {
  let value: unknown = raw
  if (typeof raw === 'string') {
    try {
      value = JSON.parse(raw) as unknown
    } catch {
      return undefined
    }
  }
  if (!isRecord(value)) return undefined
  if (
    typeof value.id !== 'string' ||
    typeof value.name !== 'string' ||
    typeof value.occurred_at_ms !== 'number' ||
    !Number.isFinite(value.occurred_at_ms) ||
    !isRecord(value.payload) ||
    typeof value.payload.run_id !== 'string' ||
    typeof value.payload.task_id !== 'string' ||
    !isRecord(value.payload.details)
  ) {
    return undefined
  }

  const payload = value.payload
  const details = payload.details as Record<string, unknown>
  switch (value.name) {
    case 'agent.outputStarted':
      return value as AgentExecutionOutputEvent
    case 'agent.outputDelta':
      return typeof details.delta === 'string' ? (value as AgentExecutionOutputEvent) : undefined
    case 'agent.outputCompleted':
      return typeof details.output === 'string' ? (value as AgentExecutionOutputEvent) : undefined
    case 'agent.error':
      return typeof details.code === 'string' && typeof details.message === 'string'
        ? (value as AgentExecutionOutputEvent)
        : undefined
    default:
      return undefined
  }
}

export function parseCoreAgentEvent(raw: unknown): AgentExecutionOutputEvent | undefined {
  return parseAgentEvent(raw)
}

export function createCoreEventClient(options: CoreEventClientOptions): CoreEventClient {
  const createWebSocket = options.createWebSocket ?? ((url) => new WebSocket(url))
  const reconnect = options.reconnect ?? true
  const reconnectDelayMs = options.reconnectDelayMs ?? 1000
  const subscriptions = new Map<AgentRunId, Set<CoreAgentEventHandlers>>()
  const terminalRuns = new Set<AgentRunId>()
  let socket: Socket | null = null
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null
  let disposed = false

  function scheduleReconnect(): void {
    if (disposed || !reconnect || reconnectTimer !== null || subscriptions.size === 0) return
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null
      connect()
    }, reconnectDelayMs)
  }

  function handleEvent(raw: unknown): void {
    const event = parseAgentEvent(raw)
    if (
      !event ||
      !subscriptions.has(event.payload.run_id) ||
      terminalRuns.has(event.payload.run_id)
    )
      return

    if (event.name === 'agent.outputCompleted' || event.name === 'agent.error') {
      terminalRuns.add(event.payload.run_id)
    }
    for (const handlers of subscriptions.get(event.payload.run_id) ?? []) {
      if (event.name === 'agent.outputStarted') handlers.onOutputStarted?.(event)
      if (event.name === 'agent.outputDelta') handlers.onOutputDelta?.(event)
      if (event.name === 'agent.outputCompleted') handlers.onOutputCompleted?.(event)
      if (event.name === 'agent.error') handlers.onError?.(event)
    }
  }

  function connect(): void {
    if (disposed || !options.url || socket) return
    try {
      const next = createWebSocket(options.url)
      socket = next
      next.onmessage = (event) => handleEvent(event.data)
      next.onerror = () => undefined
      next.onclose = () => {
        if (socket === next) socket = null
        scheduleReconnect()
      }
    } catch {
      socket = null
      scheduleReconnect()
    }
  }

  function subscribeToRun(
    runId: AgentRunId,
    handlers: CoreAgentEventHandlers,
  ): CoreEventSubscription {
    if (disposed) return { dispose: () => undefined }
    let runSubscriptions = subscriptions.get(runId)
    if (!runSubscriptions) {
      runSubscriptions = new Set()
      subscriptions.set(runId, runSubscriptions)
    }
    runSubscriptions.add(handlers)
    connect()
    let active = true
    return {
      dispose: () => {
        if (!active) return
        active = false
        runSubscriptions?.delete(handlers)
        if (runSubscriptions?.size === 0) subscriptions.delete(runId)
      },
    }
  }

  return {
    connect,
    subscribeToRun,
    dispose: () => {
      if (disposed) return
      disposed = true
      subscriptions.clear()
      terminalRuns.clear()
      if (reconnectTimer !== null) clearTimeout(reconnectTimer)
      reconnectTimer = null
      const current = socket
      socket = null
      current?.close()
    },
  }
}
