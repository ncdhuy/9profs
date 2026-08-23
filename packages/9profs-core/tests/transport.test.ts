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

  it('maps assistant and skill catalog APIs without exposing Rust types', async () => {
    const requests: Array<{ input: string; method?: string; body?: string }> = []
    const transport = createCoreTransport('http://127.0.0.1:39761/', async (input, init) => {
      requests.push({ input, method: init?.method, body: init?.body })
      const data = input.endsWith('/api/assistants')
        ? []
        : input.endsWith('/api/skills')
          ? { skills: [], issues: [] }
          : {}
      return { ok: true, json: async () => ({ success: true, data }) }
    })

    await expect(transport.assistants()).resolves.toEqual([])
    await expect(transport.skills()).resolves.toEqual({ skills: [], issues: [] })
    await transport.scanSkills()
    expect(requests).toEqual([
      { input: 'http://127.0.0.1:39761/api/assistants' },
      { input: 'http://127.0.0.1:39761/api/skills' },
      { input: 'http://127.0.0.1:39761/api/skills/scan', method: 'POST', body: undefined },
    ])
  })

  it('maps agent registry list and get APIs', async () => {
    const requests: string[] = []
    const transport = createCoreTransport('http://127.0.0.1:39761', async (input) => {
      requests.push(input)
      return {
        ok: true,
        json: async () => ({
          success: true,
          data: input.endsWith('/api/agents')
            ? [
                {
                  id: 'codex',
                  name: 'Codex',
                  description: 'Future backend',
                  source: 'builtin',
                  kind: 'cli',
                  capabilities: ['cancellation'],
                  availability: 'unknown',
                  availability_reason: null,
                  enabled: true,
                  sort_order: 10,
                  version: null,
                  created_at_ms: null,
                  updated_at_ms: null,
                },
              ]
            : {
                id: 'codex',
                name: 'Codex',
                description: 'Future backend',
                source: 'builtin',
                kind: 'cli',
                capabilities: ['cancellation'],
                availability: 'unknown',
                availability_reason: null,
                enabled: true,
                sort_order: 10,
                version: null,
                created_at_ms: null,
                updated_at_ms: null,
              },
        }),
      }
    })

    await expect(transport.agents()).resolves.toHaveLength(1)
    await expect(transport.agent('codex')).resolves.toMatchObject({
      id: 'codex',
      availability: 'unknown',
    })
    expect(requests).toEqual([
      'http://127.0.0.1:39761/api/agents',
      'http://127.0.0.1:39761/api/agents/codex',
    ])
  })

  it('maps agent run creation, diagnostics, and cancellation APIs', async () => {
    const requests: Array<{ input: string; method?: string; body?: string }> = []
    const task = {
      task_id: 'task-1',
      run_id: 'run-1',
      backend_id: 'nineprofs-default',
      state: 'queued',
      created_at_ms: 1,
      updated_at_ms: 1,
      started_at_ms: null,
      completed_at_ms: null,
      failure: null,
      cancellation_requested: false,
    }
    const transport = createCoreTransport('http://127.0.0.1:39761/', async (input, init) => {
      requests.push({ input, method: init?.method, body: init?.body })
      const data = input.endsWith('/api/agent-runs')
        ? { run_id: 'run-1', task }
        : input.endsWith('/tasks')
          ? [task]
          : input.includes('/api/agent-tasks/')
            ? task
            : { run_id: 'run-1', tasks: [task] }
      return { ok: true, json: async () => ({ success: true, data }) }
    })

    await expect(
      transport.createAgentRun({ assistant_id: 'assistant-1', input: 'hello' }),
    ).resolves.toMatchObject({ run_id: 'run-1' })
    await expect(transport.agentRun('run/1')).resolves.toMatchObject({ run_id: 'run-1' })
    await expect(transport.agentRunTasks('run/1')).resolves.toHaveLength(1)
    await expect(transport.cancelAgentTask('task/1')).resolves.toMatchObject({ task_id: 'task-1' })
    expect(requests).toEqual([
      {
        input: 'http://127.0.0.1:39761/api/agent-runs',
        method: 'POST',
        body: '{"assistant_id":"assistant-1","input":"hello"}',
      },
      { input: 'http://127.0.0.1:39761/api/agent-runs/run%2F1' },
      { input: 'http://127.0.0.1:39761/api/agent-runs/run%2F1/tasks' },
      {
        input: 'http://127.0.0.1:39761/api/agent-tasks/task%2F1/cancel',
        method: 'POST',
      },
    ])
  })
})
