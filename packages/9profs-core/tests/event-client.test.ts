import { describe, expect, it } from 'vitest'
import { createCoreEventClient, parseCoreAgentEvent } from '../src/event-client'

class FakeSocket {
  onmessage: ((event: { data: unknown }) => void) | null = null
  onerror: (() => void) | null = null
  onclose: (() => void) | null = null
  closed = false

  close(): void {
    this.closed = true
    this.onclose?.()
  }

  deliver(payload: unknown): void {
    this.onmessage?.({ data: typeof payload === 'string' ? payload : JSON.stringify(payload) })
  }
}

function event(name: string, details: Record<string, unknown>, runId = 'run-1') {
  return {
    id: `${name}-1`,
    name,
    occurred_at_ms: 1,
    payload: { run_id: runId, task_id: 'task-1', details },
  }
}

describe('Core event client', () => {
  it('parses ordered output events, filters runs, and emits one terminal event', () => {
    const socket = new FakeSocket()
    const received: string[] = []
    const client = createCoreEventClient({
      url: 'ws://core/ws',
      createWebSocket: () => socket as unknown as WebSocket,
      reconnect: false,
    })
    client.subscribeToRun('run-1', {
      onOutputStarted: () => received.push('started'),
      onOutputDelta: (value) => received.push(value.payload.details.delta),
      onOutputCompleted: () => received.push('completed'),
      onError: () => received.push('error'),
      onToolStarted: (value) => received.push(`tool-start:${value.payload.details.tool}`),
      onToolCompleted: (value) => received.push(`tool-done:${value.payload.details.tool}`),
    })

    socket.deliver(event('agent.outputStarted', {}))
    socket.deliver(event('agent.outputDelta', { delta: 'one' }))
    socket.deliver(event('agent.outputDelta', { delta: 'two' }))
    socket.deliver(
      event('agent.toolStarted', { tool_call_id: 'call-1', name: 'document.inspect_active' }),
    )
    socket.deliver(
      event('agent.toolCompleted', {
        tool_call_id: 'call-1',
        name: 'document.inspect_active',
        is_error: false,
      }),
    )
    socket.deliver(event('agent.outputCompleted', { output: 'onetwo' }))
    socket.deliver(event('agent.outputCompleted', { output: 'duplicate' }))
    socket.deliver(event('agent.error', { code: 'late', message: 'late' }))
    socket.deliver(event('agent.outputDelta', { delta: 'other' }, 'run-2'))

    expect(received).toEqual([
      'started',
      'one',
      'two',
      'tool-start:document.inspect_active',
      'tool-done:document.inspect_active',
      'completed',
    ])
  })

  it('delivers error as terminal event and ignores malformed envelopes', () => {
    const socket = new FakeSocket()
    const errors: string[] = []
    const client = createCoreEventClient({
      url: 'ws://core/ws',
      createWebSocket: () => socket as unknown as WebSocket,
      reconnect: false,
    })
    client.subscribeToRun('run-1', { onError: (value) => errors.push(value.payload.details.code) })

    socket.deliver('{not-json')
    socket.deliver({ name: 'agent.outputDelta' })
    socket.deliver({
      ...event('agent.error', { code: 'malformed', message: 'ignored' }),
      occurred_at_ms: 'bad',
    })
    socket.deliver(event('agent.error', { code: 'failed', message: 'nope' }))
    socket.deliver(event('agent.error', { code: 'duplicate', message: 'ignored' }))

    expect(parseCoreAgentEvent('{not-json')).toBeUndefined()
    expect(errors).toEqual(['failed'])
  })

  it('disposes subscriptions and reconnects without creating a run', async () => {
    const sockets: FakeSocket[] = []
    const client = createCoreEventClient({
      url: 'ws://core/ws',
      createWebSocket: () => {
        const socket = new FakeSocket()
        sockets.push(socket)
        return socket as unknown as WebSocket
      },
      reconnectDelayMs: 0,
    })
    const subscription = client.subscribeToRun('run-1', {})
    sockets[0].close()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(sockets).toHaveLength(2)

    subscription.dispose()
    client.dispose()
    expect(sockets[1].closed).toBe(true)
    sockets[1].deliver(event('agent.outputStarted', {}))
  })
})
