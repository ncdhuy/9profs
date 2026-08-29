import type {
  ApprovedDocumentChangeSet,
  DocumentInspection,
  DocumentMutationResult,
} from '@genoffice/document-gateway'
import { GenOfficeDocsMutationError, type GenOfficeDocsAdapter } from './docs'

const OPEN = 1
const MAX_RECONNECT_DELAY_MS = 30_000

export interface GenOfficeDocsBridgeOptions {
  readonly adapter: GenOfficeDocsAdapter
  /** Supplied by the host; omitted URL keeps bridge disabled. */
  readonly websocketUrl: string
  /** Sent only inside initial register handshake. */
  readonly sessionSecret?: string
  readonly createWebSocket?: (url: string) => WebSocket
  readonly reconnect?: boolean
  readonly reconnectDelayMs?: number
}

type CoreBridgeMessage =
  | { readonly type: 'registered'; readonly documentId: string; readonly version: number }
  | {
      readonly type: 'inspect'
      readonly requestId: string
      readonly documentId: string
    }
  | {
      readonly type: 'commitApprovedChangeSet'
      readonly requestId: string
      readonly documentId: string
      readonly changeSet: ApprovedDocumentChangeSet
    }
  | { readonly type: 'error'; readonly requestId?: string; readonly code: string; readonly message: string }

function serializeError(error: unknown): { code: string; message: string } {
  if (error instanceof GenOfficeDocsMutationError) {
    return { code: error.code, message: error.message }
  }
  return {
    code: 'bridge-error',
    message: error instanceof Error ? error.message : 'active Docs bridge request failed',
  }
}

export class GenOfficeDocsBridgeClient {
  private readonly adapter: GenOfficeDocsAdapter
  private readonly websocketUrl: string
  private readonly sessionSecret?: string
  private readonly createWebSocket: (url: string) => WebSocket
  private readonly reconnect: boolean
  private readonly reconnectDelayMs: number
  private reconnectAttempt = 0
  private readonly unsubscribeVersion: () => void
  private socket: WebSocket | null = null
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private disposed = false
  private registered = false

  constructor(options: GenOfficeDocsBridgeOptions) {
    this.adapter = options.adapter
    this.websocketUrl = options.websocketUrl
    this.sessionSecret = options.sessionSecret
    this.createWebSocket = options.createWebSocket ?? ((url) => new WebSocket(url))
    this.reconnect = options.reconnect ?? true
    this.reconnectDelayMs = Math.max(1, options.reconnectDelayMs ?? 1000)
    this.unsubscribeVersion = this.adapter.versionTracker.subscribe((version) => {
      if (this.registered) {
        this.send({
          type: 'versionChanged',
          documentId: this.adapter.documentId,
          version,
        })
      }
    })
  }

  connect(): void {
    if (this.disposed || !this.websocketUrl || this.socket) return
    try {
      const socket = this.createWebSocket(this.websocketUrl)
      this.socket = socket
      socket.onopen = () => {
        if (this.disposed || this.socket !== socket) return
        this.reconnectAttempt = 0
        this.send({
          type: 'register',
          protocolVersion: '1',
          documentId: this.adapter.documentId,
          documentType: 'docx',
          version: this.adapter.versionTracker.version,
          capabilities: ['inspect', 'commitApprovedChangeSet'],
          ...(this.sessionSecret === undefined
            ? {}
            : { auth: { sessionSecret: this.sessionSecret } }),
        })
      }
      socket.onmessage = (event) => {
        if (this.socket === socket) this.handleMessage(event.data)
      }
      socket.onerror = () => undefined
      socket.onclose = () => {
        if (this.socket !== socket) return
        this.socket = null
        this.registered = false
        this.scheduleReconnect()
      }
    } catch {
      this.socket = null
      this.scheduleReconnect()
    }
  }

  dispose(): void {
    this.disposed = true
    this.registered = false
    this.unsubscribeVersion()
    if (this.reconnectTimer !== null) clearTimeout(this.reconnectTimer)
    this.reconnectTimer = null
    const socket = this.socket
    this.socket = null
    socket?.close()
  }

  private scheduleReconnect(): void {
    if (this.disposed || !this.reconnect || this.reconnectTimer !== null) return
    const delay = Math.min(
      this.reconnectDelayMs * 2 ** this.reconnectAttempt,
      MAX_RECONNECT_DELAY_MS,
    )
    this.reconnectAttempt = Math.min(this.reconnectAttempt + 1, 30)
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null
      this.connect()
    }, delay)
  }

  private handleMessage(raw: unknown): void {
    if (typeof raw !== 'string') return
    let message: CoreBridgeMessage
    try {
      message = JSON.parse(raw) as CoreBridgeMessage
    } catch {
      return
    }
    if (message.type === 'registered') {
      if (message.documentId === this.adapter.documentId) this.registered = true
      return
    }
    if (message.type === 'error') return
    if (message.documentId !== this.adapter.documentId) return
    if (message.type === 'inspect') {
      void this.inspect(message.requestId)
      return
    }
    if (message.type === 'commitApprovedChangeSet') {
      void this.commit(message.requestId, message.changeSet)
    }
  }

  private async inspect(requestId: string): Promise<void> {
    try {
      const inspection = await this.adapter.inspector.inspect({ documentId: this.adapter.documentId })
      this.respond(requestId, { kind: 'inspection', inspection })
    } catch (error) {
      this.respond(requestId, { kind: 'error', ...serializeError(error) })
    }
  }

  private async commit(requestId: string, changeSet: ApprovedDocumentChangeSet): Promise<void> {
    try {
      const result = await this.adapter.mutationGateway.commit(changeSet)
      this.respond(requestId, { kind: 'mutation', result })
    } catch (error) {
      this.respond(requestId, { kind: 'error', ...serializeError(error) })
    }
  }

  private respond(
    requestId: string,
    response:
      | { readonly kind: 'inspection'; readonly inspection: DocumentInspection }
      | { readonly kind: 'mutation'; readonly result: DocumentMutationResult }
      | { readonly kind: 'error'; readonly code: string; readonly message: string },
  ): void {
    this.send({
      type: 'response',
      requestId,
      documentId: this.adapter.documentId,
      response,
    })
  }

  private send(message: unknown): void {
    if (this.socket?.readyState !== OPEN) return
    this.socket.send(JSON.stringify(message))
  }
}
