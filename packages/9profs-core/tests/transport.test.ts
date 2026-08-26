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

  it('maps safe active-document and read-only proposal APIs', async () => {
    const requests: Array<{ input: string; method?: string }> = []
    const transport = createCoreTransport('http://127.0.0.1:39761/', async (input, init) => {
      requests.push({ input, method: init?.method })
      const data = input.includes('/api/document-proposals/')
        ? {
            proposalId: 'proposal-1',
            changeSetId: 'proposal-1',
            documentId: 'doc-1',
            authority: 'genoffice-active',
            baseVersion: 5,
            status: 'proposed',
            freshness: 'fresh',
            availability: 'available',
            currentVersion: 5,
            createdAtMs: 1,
            changes: [],
          }
        : input.includes('/api/document-proposals')
          ? []
          : input.endsWith('/api/documents/doc-1')
            ? {
                documentId: 'doc-1',
                documentType: 'docx',
                authority: 'genoffice-active',
                version: 5,
                capabilities: ['inspect', 'commitApprovedChangeSet'],
                availability: 'available',
              }
            : []
      return { ok: true, json: async () => ({ success: true, data }) }
    })

    await expect(transport.activeDocuments()).resolves.toEqual([])
    await expect(transport.activeDocument('doc-1')).resolves.toMatchObject({ documentId: 'doc-1' })
    await expect(transport.documentProposals()).resolves.toEqual([])
    await expect(transport.documentProposals('doc/1')).resolves.toEqual([])
    await expect(transport.documentProposal('proposal/1')).resolves.toMatchObject({
      proposalId: 'proposal-1',
      status: 'proposed',
    })
    expect(requests).toEqual([
      { input: 'http://127.0.0.1:39761/api/documents', method: undefined },
      { input: 'http://127.0.0.1:39761/api/documents/doc-1', method: undefined },
      { input: 'http://127.0.0.1:39761/api/document-proposals', method: undefined },
      {
        input: 'http://127.0.0.1:39761/api/document-proposals?documentId=doc%2F1',
        method: undefined,
      },
      {
        input: 'http://127.0.0.1:39761/api/document-proposals/proposal%2F1',
        method: undefined,
      },
    ])
  })

  it('maps MCP configuration, diagnostics, and tool APIs without exposing secrets', async () => {
    const requests: Array<{ input: string; method?: string; body?: string }> = []
    const server = {
      id: 'local',
      name: 'Local',
      description: 'fixture',
      enabled: false,
      startup_timeout_ms: 1000,
      transport: { type: 'stdio', command: 'fixture', args: [], env_keys: ['TOKEN'] },
      status: 'disconnected',
      last_connected: null,
      error: null,
      supports_resources: false,
      tools: [],
      created_at_ms: 1,
      updated_at_ms: 1,
    }
    const transport = createCoreTransport('http://127.0.0.1:39761/', async (input, init) => {
      requests.push({ input, method: init?.method, body: init?.body })
      const data = input.endsWith('/tools')
        ? []
        : input.endsWith('/test')
          ? { success: true, tool_count: 1, supports_resources: false, error: null }
          : input.endsWith('/api/mcp/servers') && init?.method === undefined
            ? [server]
            : server
      return { ok: true, json: async () => ({ success: true, data }) }
    })

    await expect(transport.mcpServers()).resolves.toEqual([server])
    await expect(transport.mcpServer('local')).resolves.toEqual(server)
    await expect(
      transport.createMcpServer({
        name: 'Local',
        transport: { type: 'stdio', command: 'fixture', env: { TOKEN: 'secret' } },
      }),
    ).resolves.toEqual(server)
    await expect(transport.testMcpServer('local')).resolves.toMatchObject({ success: true })
    await expect(transport.mcpTools('local')).resolves.toEqual([])
    expect(requests.map(({ input, method }) => [input, method])).toEqual([
      ['http://127.0.0.1:39761/api/mcp/servers', undefined],
      ['http://127.0.0.1:39761/api/mcp/servers/local', undefined],
      ['http://127.0.0.1:39761/api/mcp/servers', 'POST'],
      ['http://127.0.0.1:39761/api/mcp/servers/local/test', 'POST'],
      ['http://127.0.0.1:39761/api/mcp/servers/local/tools', undefined],
    ])
    expect(requests[2].body).toContain('secret')
  })
})
