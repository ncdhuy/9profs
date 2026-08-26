import { describe, expect, it } from 'vitest'
import {
  createGenOfficeDocsAdapter,
  DOCS_COMMAND_ENVELOPE,
  GenOfficeDocsBridgeClient,
  type GenOfficeDocsRuntime,
} from '../src'

class FakeSocket {
  readyState = 0
  sent: string[] = []
  onopen: (() => void) | null = null
  onmessage: ((event: { data: unknown }) => void) | null = null
  onerror: (() => void) | null = null
  onclose: (() => void) | null = null

  send(payload: string): void {
    this.sent.push(payload)
  }

  close(): void {
    this.readyState = 3
    this.onclose?.()
  }

  open(): void {
    this.readyState = 1
    this.onopen?.()
  }

  deliver(payload: unknown): void {
    this.onmessage?.({ data: JSON.stringify(payload) })
  }
}

function harness() {
  const listeners = new Set<(transaction: { docChanged: boolean }) => void>()
  const runtime: GenOfficeDocsRuntime = {
    subscribeToTransactions(listener) {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    buildDocumentContext: () => ({ text: 'hello' }),
    getSelectionContext: () => ({ from: 1, to: 1, empty: true }),
    executeCommands(commands) {
      if (commands.length > 0) for (const listener of listeners) listener({ docChanged: true })
      return { ok: true, results: [{ changed: commands.length }], summary: 'applied' }
    },
  }
  return {
    adapter: createGenOfficeDocsAdapter({ documentId: 'doc-1', runtime }),
    emit(docChanged: boolean) {
      for (const listener of listeners) listener({ docChanged })
    },
  }
}

function approved(baseVersion: number) {
  return {
    id: 'change-1',
    status: 'approved' as const,
    target: { kind: 'genoffice-active' as const, documentId: 'doc-1', writeAuthority: 'genoffice' as const },
    baseVersion,
    changes: [{ type: DOCS_COMMAND_ENVELOPE, payload: { commands: [{ replaceAllText: {} }] } }],
    approval: { approvedBy: 'test', approvedAt: '2026-08-26T00:00:00Z' },
  }
}

function sent(socket: FakeSocket): unknown[] {
  return socket.sent.map((payload) => JSON.parse(payload))
}

describe('GenOffice Docs bridge client', () => {
  it('registers, reports meaningful versions, inspects, commits, and preserves stale conflicts', async () => {
    const { adapter } = harness()
    const socket = new FakeSocket()
    const bridge = new GenOfficeDocsBridgeClient({
      adapter,
      websocketUrl: 'ws://test/ws/documents',
      createWebSocket: () => socket as unknown as WebSocket,
      reconnect: false,
    })
    bridge.connect()
    socket.open()
    expect(sent(socket)[0]).toMatchObject({
      type: 'register',
      documentId: 'doc-1',
      documentType: 'docx',
      version: 0,
    })
    socket.deliver({ type: 'registered', documentId: 'doc-1', version: 0 })

    socket.deliver({ type: 'inspect', requestId: 'inspect-1', documentId: 'doc-1' })
    await Promise.resolve()
    expect(sent(socket).at(-1)).toMatchObject({
      type: 'response',
      requestId: 'inspect-1',
      response: { kind: 'inspection', inspection: { documentId: 'doc-1', version: 0 } },
    })

    adapter.versionTracker.reset(1)
    socket.deliver({
      type: 'commitApprovedChangeSet',
      requestId: 'commit-stale',
      documentId: 'doc-1',
      changeSet: approved(0),
    })
    await Promise.resolve()
    expect(sent(socket).at(-1)).toMatchObject({
      requestId: 'commit-stale',
      response: { kind: 'mutation', result: { status: 'conflict', reason: 'stale-version' } },
    })

    socket.deliver({ type: 'commitApprovedChangeSet', requestId: 'commit-ok', documentId: 'doc-1', changeSet: approved(1) })
    await Promise.resolve()
    expect(sent(socket).at(-1)).toMatchObject({
      requestId: 'commit-ok',
      response: { kind: 'mutation', result: { status: 'applied', previousVersion: 1, newVersion: 2 } },
    })
    bridge.dispose()
    adapter.dispose()
  })

  it('sends no selection-only version message, serializes errors, and disposes without reconnect', async () => {
    const { adapter, emit } = harness()
    const socket = new FakeSocket()
    const bridge = new GenOfficeDocsBridgeClient({
      adapter,
      websocketUrl: 'ws://test/ws/documents',
      createWebSocket: () => socket as unknown as WebSocket,
      reconnect: false,
    })
    bridge.connect()
    socket.open()
    socket.deliver({ type: 'registered', documentId: 'doc-1', version: 0 })
    const initialCount = socket.sent.length
    emit(false)
    expect(socket.sent.length).toBe(initialCount)
    emit(true)
    expect(sent(socket).at(-1)).toMatchObject({ type: 'versionChanged', documentId: 'doc-1', version: 1 })
    socket.deliver({ type: 'commitApprovedChangeSet', requestId: 'bad', documentId: 'doc-1', changeSet: { ...approved(0), status: 'proposed' } })
    await Promise.resolve()
    expect(sent(socket).at(-1)).toMatchObject({
      requestId: 'bad',
      response: { kind: 'error', code: 'invalid-status' },
    })
    expect(socket.sent.length).toBeGreaterThan(initialCount)
    bridge.dispose()
    expect(socket.readyState).toBe(3)
    adapter.dispose()
  })

  it('reconnects the same document session without resetting its version', async () => {
    const { adapter } = harness()
    adapter.resetVersion(5)
    const sockets: FakeSocket[] = []
    const bridge = new GenOfficeDocsBridgeClient({
      adapter,
      websocketUrl: 'ws://test/ws/documents',
      createWebSocket: () => {
        const socket = new FakeSocket()
        sockets.push(socket)
        return socket as unknown as WebSocket
      },
      reconnect: true,
      reconnectDelayMs: 0,
    })
    bridge.connect()
    sockets[0].open()
    sockets[0].deliver({ type: 'registered', documentId: 'doc-1', version: 5 })
    sockets[0].close()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(sockets).toHaveLength(2)
    sockets[1].open()
    expect(sent(sockets[1])[0]).toMatchObject({ type: 'register', documentId: 'doc-1', version: 5 })
    bridge.dispose()
    adapter.dispose()
  })
})
