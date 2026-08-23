import { describe, expect, it } from 'vitest'
import { createCoreTransport } from '../src/transport'

describe('Core transport boundary', () => {
  it('maps stable HTTP and WebSocket endpoints without Rust dependencies', async () => {
    const requests: string[] = []
    const transport = createCoreTransport('http://127.0.0.1:39761/', async (input) => {
      requests.push(input)
      return {
        ok: true,
        async json() {
          return input.endsWith('/health')
            ? { success: true, data: { status: 'ok', service: '9profs-core' } }
            : {
                success: true,
                data: {
                  service: '9profs-core',
                  version: '0.1.0',
                  protocol_version: '1',
                  capabilities: ['health', 'runtime', 'realtime'],
                },
              }
        },
      }
    })

    await expect(transport.health()).resolves.toEqual({ status: 'ok', service: '9profs-core' })
    await expect(transport.runtime()).resolves.toMatchObject({ protocol_version: '1' })
    expect(transport.websocketUrl()).toBe('ws://127.0.0.1:39761/ws')
    expect(requests).toEqual([
      'http://127.0.0.1:39761/api/health',
      'http://127.0.0.1:39761/api/runtime',
    ])
  })
})
