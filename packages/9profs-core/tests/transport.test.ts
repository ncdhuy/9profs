import { describe, expect, it } from 'vitest'
import { extractResearchPdfPages } from '../src/research-pdf'
import { createCoreTransport } from '../src/transport'

describe('Core transport boundary', () => {
  it('preserves typed Core errors for missing active documents', async () => {
    const transport = createCoreTransport('http://127.0.0.1:39761', async () => ({
      ok: false,
      status: 404,
      json: async () => ({
        success: false,
        error: 'active document not found: doc-1',
        code: 'not_found',
      }),
    }))

    await expect(transport.activeDocument('doc-1')).rejects.toMatchObject({
      name: 'CoreTransportError',
      path: '/api/documents/doc-1',
      status: 404,
      code: 'not_found',
    })
  })

  it('maps manuscript citation review start and read-model endpoints', async () => {
    const requests: Array<{ input: string; method?: string; body?: string }> = []
    const transport = createCoreTransport('http://127.0.0.1:39761', async (input, init) => {
      requests.push({
        input,
        method: init?.method,
        body: typeof init?.body === 'string' ? init.body : undefined,
      })
      return { ok: true, json: async () => ({ success: true, data: [] }) }
    })

    await transport.startManuscriptCitationReview('case-1', {
      manuscriptSourceId: 'source-1',
      documentId: 'doc-1',
      documentVersion: 3,
      citations: [
        {
          format: 'zotero',
          renderedText: '(source)',
          blockId: 'block-1',
          start: 6,
          end: 14,
          targets: [
            {
              ordinal: 0,
              referenceKey: 'source-key',
              citedLocator: null,
              zotero: { itemId: 'item-1', uris: [] },
            },
          ],
        },
      ],
      blocks: [
        {
          blockId: 'block-1',
          text: 'Claim (source)',
          citations: [{ start: 6, end: 14, renderedText: '(source)' }],
        },
      ],
    })
    await transport.manuscriptCitationReview('review-1')
    await transport.manuscriptCitationReviewItems('review-1')

    const startBody = JSON.parse(requests[0].body ?? '{}') as Record<string, unknown>
    expect(startBody).toMatchObject({
      manuscriptSourceId: 'source-1',
      documentId: 'doc-1',
      documentVersion: 3,
    })
    expect(startBody).not.toHaveProperty('citationSyncRunId')
    expect(startBody).not.toHaveProperty('referenceCatalogRunId')
    expect(startBody).not.toHaveProperty('referenceResolutionRunId')
    expect(startBody).not.toHaveProperty('claimExtractionRunId')

    expect(requests).toEqual([
      {
        input: 'http://127.0.0.1:39761/api/research/cases/case-1/manuscript-citation-reviews',
        method: 'POST',
        body: JSON.stringify({
          manuscriptSourceId: 'source-1',
          documentId: 'doc-1',
          documentVersion: 3,
          citations: [
            {
              format: 'zotero',
              renderedText: '(source)',
              blockId: 'block-1',
              start: 6,
              end: 14,
              targets: [
                {
                  ordinal: 0,
                  referenceKey: 'source-key',
                  citedLocator: null,
                  zotero: { itemId: 'item-1', uris: [] },
                },
              ],
            },
          ],
          blocks: [
            {
              blockId: 'block-1',
              text: 'Claim (source)',
              citations: [{ start: 6, end: 14, renderedText: '(source)' }],
            },
          ],
        }),
      },
      {
        input: 'http://127.0.0.1:39761/api/research/manuscript-citation-reviews/review-1',
        body: undefined,
      },
      {
        input: 'http://127.0.0.1:39761/api/research/manuscript-citation-reviews/review-1/items',
        body: undefined,
      },
    ])
  })

  it('maps whole-manuscript claim inventory through the trusted start and read endpoints', async () => {
    const requests: Array<{ input: string; method?: string; body?: string }> = []
    const transport = createCoreTransport('http://127.0.0.1:39761', async (input, init) => {
      requests.push({
        input,
        method: init?.method,
        body: typeof init?.body === 'string' ? init.body : undefined,
      })
      return { ok: true, json: async () => ({ success: true, data: [] }) }
    })
    const input = {
      manuscriptSourceId: 'source-1',
      documentId: 'doc-1',
      documentVersion: 7,
      blocks: [
        {
          blockId: 'b1',
          blockOrdinal: 0,
          blockKind: 'paragraph' as const,
          text: 'A claim [1].',
          citations: [{ start: 9, end: 12, renderedText: '[1]' }],
        },
        {
          blockId: 'b2',
          blockOrdinal: 1,
          blockKind: 'paragraph' as const,
          text: 'An uncited claim.',
          citations: [],
        },
      ],
    }

    await transport.startManuscriptClaimInventory('case-1', input)
    await transport.manuscriptClaimInventory('inventory-1')
    await transport.manuscriptClaimInventoryItems('inventory-1')
    await transport.manuscriptClaimInventoryCoverage('inventory-1')

    expect(requests).toEqual([
      {
        input: 'http://127.0.0.1:39761/api/research/cases/case-1/manuscript-claim-inventories',
        method: 'POST',
        body: JSON.stringify(input),
      },
      {
        input: 'http://127.0.0.1:39761/api/research/manuscript-claim-inventories/inventory-1',
      },
      {
        input: 'http://127.0.0.1:39761/api/research/manuscript-claim-inventories/inventory-1/items',
      },
      {
        input:
          'http://127.0.0.1:39761/api/research/manuscript-claim-inventories/inventory-1/coverage',
      },
    ])
  })

  it('maps manuscript reference resolution and confirmation endpoints', async () => {
    const requests: Array<{ input: string; method?: string }> = []
    const transport = createCoreTransport('http://127.0.0.1:39761', async (input, init) => {
      requests.push({ input, method: init?.method })
      return { ok: true, json: async () => ({ success: true, data: {} }) }
    })

    await transport.resolveManuscriptReferences('catalog-1')
    await transport.manuscriptReferenceResolutionRun('run-1')
    await transport.manuscriptReferenceResolutionEntries('run-1')
    await transport.manuscriptReferenceResolutionCandidates('entry-1')
    await transport.confirmManuscriptReferenceCandidate('run-1', 'entry-1', 'candidate-1')

    expect(requests).toEqual([
      {
        input:
          'http://127.0.0.1:39761/api/research/manuscript-reference-catalog-runs/catalog-1/resolution',
        method: 'POST',
      },
      {
        input: 'http://127.0.0.1:39761/api/research/manuscript-reference-resolution-runs/run-1',
      },
      {
        input:
          'http://127.0.0.1:39761/api/research/manuscript-reference-resolution-runs/run-1/entries',
      },
      {
        input:
          'http://127.0.0.1:39761/api/research/manuscript-reference-resolution-entries/entry-1/candidates',
      },
      {
        input:
          'http://127.0.0.1:39761/api/research/manuscript-reference-resolution-runs/run-1/entries/entry-1/candidates/candidate-1/confirm',
        method: 'POST',
      },
    ])
  })

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

  it('maps Docs conversation APIs with trusted writes and safe metadata', async () => {
    const requests: Array<{
      input: string
      method?: string
      headers?: Record<string, string>
      body?: string
    }> = []
    const conversation = {
      conversationId: 'docs-conversation-1',
      assistantId: 'document-foundation',
      documentId: 'doc-a',
      state: 'idle',
      turnCount: 1,
      createdAtMs: 1,
      updatedAtMs: 2,
    }
    const transport = createCoreTransport(
      'http://127.0.0.1:39761',
      async (input, init) => {
        requests.push({ input, method: init?.method, headers: init?.headers, body: init?.body })
        const data = input.endsWith('/runs')
          ? { run_id: 'run-1', task: { task_id: 'task-1' } }
          : conversation
        return { ok: true, json: async () => ({ success: true, data }) }
      },
      { sessionSecret: 'test-only-secret' },
    )

    await expect(
      transport.createDocumentAgentConversation({
        assistantId: 'document-foundation',
        documentId: 'doc-a',
      }),
    ).resolves.toEqual(conversation)
    await expect(
      transport.createDocumentAgentConversationRun('docs-conversation-1', { input: 'continue' }),
    ).resolves.toMatchObject({ run_id: 'run-1' })
    await expect(transport.documentAgentConversation('docs-conversation-1')).resolves.toEqual(
      conversation,
    )

    expect(requests).toEqual([
      {
        input: 'http://127.0.0.1:39761/api/document-agent-conversations',
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          'x-nineprofs-session-secret': 'test-only-secret',
        },
        body: '{"assistant_id":"document-foundation","document_id":"doc-a"}',
      },
      {
        input: 'http://127.0.0.1:39761/api/document-agent-conversations/docs-conversation-1/runs',
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          'x-nineprofs-session-secret': 'test-only-secret',
        },
        body: '{"input":"continue"}',
      },
      {
        input: 'http://127.0.0.1:39761/api/document-agent-conversations/docs-conversation-1',
      },
    ])
    expect(JSON.stringify(conversation)).not.toMatch(/secret|credential|tool|backend|session/i)
  })

  it('maps safe Docs Agent profile readiness without provider secrets', async () => {
    const requests: string[] = []
    const profiles = [
      {
        defaultAssistantId: 'document-foundation',
        readiness: 'ready',
        backendId: 'nineprofs-default',
        assistantAvailability: 'available',
        backendAvailability: 'available',
        providerReady: true,
        capabilities: [
          'document.list_active',
          'document.inspect_active',
          'document.propose_active_changes',
          'activeDocsAgentRun',
        ],
        supportsActiveDocsRuns: true,
      },
      {
        defaultAssistantId: 'document-foundation',
        readiness: 'provider_not_configured',
        reason: 'Core agent provider is not configured',
        backendId: 'nineprofs-default',
        assistantAvailability: 'available',
        backendAvailability: 'unavailable',
        providerReady: false,
        capabilities: [
          'document.list_active',
          'document.inspect_active',
          'document.propose_active_changes',
          'activeDocsAgentRun',
        ],
        supportsActiveDocsRuns: false,
      },
    ]
    const transport = createCoreTransport('http://127.0.0.1:39761/', async (input) => {
      requests.push(input)
      return { ok: true, json: async () => ({ success: true, data: profiles.shift() }) }
    })

    await expect(transport.documentAgentProfile()).resolves.toMatchObject({
      defaultAssistantId: 'document-foundation',
      readiness: 'ready',
      supportsActiveDocsRuns: true,
    })
    const unavailable = await transport.documentAgentProfile()
    expect(unavailable).toMatchObject({
      readiness: 'provider_not_configured',
      reason: 'Core agent provider is not configured',
      providerReady: false,
    })
    expect(JSON.stringify(unavailable)).not.toMatch(/api[_-]?key|secret|credential/i)
    expect(requests).toEqual([
      'http://127.0.0.1:39761/api/document-agent-profile',
      'http://127.0.0.1:39761/api/document-agent-profile',
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

  it('creates authenticated active Docs runs without exposing tool authorization', async () => {
    const requests: Array<{
      input: string
      method?: string
      headers?: Record<string, string>
      body?: string
    }> = []
    const transport = createCoreTransport(
      'http://127.0.0.1:39761/',
      async (input, init) => {
        requests.push({ input, method: init?.method, headers: init?.headers, body: init?.body })
        return {
          ok: true,
          json: async () => ({
            success: true,
            data: {
              run_id: 'run-docs-1',
              task: {
                task_id: 'task-docs-1',
                run_id: 'run-docs-1',
                backend_id: 'nineprofs-default',
                state: 'queued',
                created_at_ms: 1,
                updated_at_ms: 1,
                started_at_ms: null,
                completed_at_ms: null,
                failure: null,
                cancellation_requested: false,
              },
              context: { kind: 'activeDocs', documentId: 'doc-a' },
            },
          }),
        }
      },
      { sessionSecret: 'test-only-secret' },
    )

    await expect(
      transport.createActiveDocsAgentRun({
        assistantId: 'assistant-1',
        documentId: 'doc-a',
        input: 'inspect this document',
      }),
    ).resolves.toMatchObject({ context: { kind: 'activeDocs', documentId: 'doc-a' } })
    expect(requests).toEqual([
      {
        input: 'http://127.0.0.1:39761/api/document-agent-runs',
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          'x-nineprofs-session-secret': 'test-only-secret',
        },
        body: '{"assistant_id":"assistant-1","document_id":"doc-a","input":"inspect this document"}',
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

  it('keeps trusted proposal decisions on a dedicated authenticated boundary', async () => {
    const requests: Array<{
      input: string
      method?: string
      headers?: Record<string, string>
      body?: string
    }> = []
    const transport = createCoreTransport(
      'http://127.0.0.1:39761/',
      async (input, init) => {
        requests.push({ input, method: init?.method, headers: init?.headers, body: init?.body })
        return {
          ok: true,
          json: async () => ({
            success: true,
            data: { proposalId: 'proposal-1', status: 'applied' },
          }),
        }
      },
      { sessionSecret: 'test-only-secret' },
    )

    await transport.approveDocumentProposal('proposal/1', 'looks good')
    await transport.rejectDocumentProposal('proposal/2')
    await transport.retryDocumentProposal('proposal/3')

    expect(requests).toEqual([
      {
        input: 'http://127.0.0.1:39761/api/document-proposals/proposal%2F1/approve',
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          'x-nineprofs-session-secret': 'test-only-secret',
        },
        body: '{"note":"looks good"}',
      },
      {
        input: 'http://127.0.0.1:39761/api/document-proposals/proposal%2F2/reject',
        method: 'POST',
        headers: { 'x-nineprofs-session-secret': 'test-only-secret' },
        body: undefined,
      },
      {
        input: 'http://127.0.0.1:39761/api/document-proposals/proposal%2F3/retry',
        method: 'POST',
        headers: { 'x-nineprofs-session-secret': 'test-only-secret' },
        body: undefined,
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

  it('keeps research reads transport-neutral and research writes trusted', async () => {
    const requests: Array<{
      input: string
      method?: string
      headers?: Record<string, string>
    }> = []
    const transport = createCoreTransport(
      'http://127.0.0.1:39761/',
      async (input, init) => {
        requests.push({ input, method: init?.method, headers: init?.headers })
        return {
          ok: true,
          json: async () => ({ success: true, data: { caseId: 'case-1' } }),
        }
      },
      { sessionSecret: 'research-secret' },
    )

    await transport.researchCases()
    await transport.researchCase('case/1')
    await transport.createResearchCase({ title: 'Review' })
    await transport.researchSources('case-1')
    await transport.createResearchSource({
      researchCaseId: 'case-1',
      kind: 'manuscript',
      label: 'Draft',
    })
    await transport.researchSnapshots('source-1')
    await transport.captureResearchSourceSnapshot({
      sourceId: 'source-1',
      content: 'captured',
      captureMethod: 'uploaded_artifact',
      origin: { kind: 'uploaded_artifact', artifact_id: 'artifact-1', revision_id: null },
    })
    await transport.researchEvidence('case-1', 'snapshot-1')
    await transport.researchEvidenceById('evidence-1')
    await transport.createResearchEvidence({
      researchCaseId: 'case-1',
      sourceSnapshotId: 'snapshot-1',
      verbatimExcerpt: 'exact',
      locator: { kind: 'text_range', start: 0, end: 5 },
      captureMethod: 'uploaded_artifact',
    })
    await transport.researchClaims('case-1')
    await transport.researchClaim('claim-1')
    await transport.createResearchClaim({
      researchCaseId: 'case-1',
      text: 'Claim',
      origin: { kind: 'user' },
    })
    await transport.claimEvidenceLinks('case-1', 'claim-1', 'evidence-1')
    await transport.claimEvidenceLink('link-1')
    await transport.createClaimEvidenceLink({
      researchCaseId: 'case-1',
      claimId: 'claim-1',
      evidenceId: 'evidence-1',
      relation: 'supports',
      assessmentMethod: 'human',
    })

    expect(requests[1].input).toContain('/api/research/cases/case%2F1')
    expect(requests[3].input).toContain('researchCaseId=case-1')
    expect(requests[5].input).toContain('sourceId=source-1')
    expect(requests.filter(({ method }) => method === 'POST')).toHaveLength(6)
    for (const request of requests.filter(({ method }) => method === 'POST')) {
      expect(request.headers).toMatchObject({
        'content-type': 'application/json',
        'x-nineprofs-session-secret': 'research-secret',
      })
    }
  })

  it('maps streamed reference PDF ingestion and exact evidence APIs', async () => {
    const requests: Array<{
      input: string
      init?: {
        method?: string
        headers?: Record<string, string>
        body?: string
        rawBody?: Uint8Array
      }
    }> = []
    const transport = createCoreTransport(
      'http://127.0.0.1:39761/',
      async (input, init) => {
        requests.push({ input, init })
        return {
          ok: true,
          json: async () => ({ success: true, data: {} }),
        }
      },
      { sessionSecret: 'research-secret' },
    )
    const bytes = new Uint8Array([37, 80, 68, 70, 45, 49])

    await transport.ingestReferencePdf('case/1', bytes, {
      filename: 'reference.pdf',
      label: 'Reference',
    })
    await transport.recordResearchPdfExtraction('snapshot/1', {
      extractor: 'pdfjs',
      extractorVersion: '4.0.0',
      pageCount: 1,
      status: 'ready',
      pages: [{ page: 1, text: 'Evidence' }],
    })
    await transport.researchPdfExtraction('extraction/1')
    await transport.researchPdfExtractions('snapshot/1')
    await transport.latestPdfExtraction('snapshot/1')
    await transport.researchPdfPages('extraction/1', { startPage: 51, limit: 25 })
    await transport.researchPdfPage('extraction/1', 1)
    await transport.captureResearchPdfEvidence({
      researchCaseId: 'case-1',
      sourceSnapshotId: 'snapshot-1',
      extractionId: 'extraction-1',
      page: 1,
      start: 0,
      end: 8,
    })

    expect(requests[0]).toEqual({
      input: 'http://127.0.0.1:39761/api/research/cases/case%2F1/reference-pdfs',
      init: {
        method: 'POST',
        headers: {
          'content-type': 'application/pdf',
          'x-nineprofs-original-filename': 'reference.pdf',
          'x-nineprofs-source-label': 'Reference',
          'x-nineprofs-session-secret': 'research-secret',
        },
        rawBody: bytes,
      },
    })
    expect(requests[1].init?.body).toBe(
      JSON.stringify({
        extractor: 'pdfjs',
        extractorVersion: '4.0.0',
        pageCount: 1,
        status: 'ready',
        pages: [{ page: 1, text: 'Evidence' }],
      }),
    )
    expect(requests[2].input).toContain('/api/research/pdf-extractions/extraction%2F1')
    expect(requests[3].input).toContain('/api/research/source-snapshots/snapshot%2F1')
    expect(requests[4].input).toContain('/api/research/snapshots/snapshot%2F1/pdf-extraction')
    expect(requests[5].input).toContain('startPage=51')
    expect(requests[5].input).toContain('limit=25')
    expect(requests[7].init?.body).not.toContain('Evidence')
    expect(requests[7].init?.body).toContain('"start":0')
    await expect(extractResearchPdfPages(new Uint8Array([1, 2, 3]))).resolves.toMatchObject({
      status: 'failed',
      pages: [],
    })
  })

  it('serializes provider-neutral exact extraction retrieval scope', async () => {
    const requests: Array<{
      input: string
      init?: { method?: string; headers?: Record<string, string>; body?: string }
    }> = []
    const transport = createCoreTransport('http://127.0.0.1:39761/', async (input, init) => {
      requests.push({ input, init })
      return { ok: true, json: async () => ({ success: true, data: [] }) }
    })

    await transport.retrieveResearchCase('case-1', {
      query: 'claim',
      topK: 5,
      scope: { kind: 'extractions', extractionIds: ['extraction-1'] },
    })

    expect(requests).toEqual([
      {
        input: 'http://127.0.0.1:39761/api/research/cases/case-1/retrieve',
        init: {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({
            query: 'claim',
            topK: 5,
            scope: { kind: 'extractions', extractionIds: ['extraction-1'] },
          }),
        },
      },
    ])
  })

  it('maps manuscript citation sync with trusted writes and bounded read surfaces', async () => {
    const requests: Array<{
      input: string
      method?: string
      headers?: Record<string, string>
      body?: string
    }> = []
    const run = {
      syncRunId: 'sync-run-1',
      researchCaseId: 'case-1',
      manuscriptSourceId: 'source-1',
      documentId: 'doc-1',
      documentVersion: 4,
      inventoryHash: { algorithm: 'sha256', value: 'abc' },
      status: 'completed',
      occurrenceCount: 1,
      createdAtMs: 1,
      completedAtMs: 1,
      failureCode: null,
    }
    const occurrence = {
      syncOccurrenceId: 'sync-occurrence-1',
      syncRunId: 'sync-run-1',
      ordinal: 0,
      citationOccurrenceId: 'citation-occurrence-1',
      documentBlockId: 'b1',
      start: 2,
      end: 7,
      format: 'zotero',
    }
    const target = {
      syncTargetId: 'sync-target-1',
      syncOccurrenceId: 'sync-occurrence-1',
      documentTargetOrdinal: 1,
      citationTargetId: 'citation-target-1',
    }
    const transport = createCoreTransport(
      'http://127.0.0.1:39761/',
      async (input, init) => {
        requests.push({ input, method: init?.method, headers: init?.headers, body: init?.body })
        const data = input.endsWith('/targets')
          ? [target]
          : input.endsWith('/occurrences')
            ? [occurrence]
            : run
        return { ok: true, json: async () => ({ success: true, data }) }
      },
      { sessionSecret: 'research-secret' },
    )
    const input = {
      documentId: 'doc-1',
      documentVersion: 4,
      citations: [
        {
          format: 'zotero' as const,
          renderedText: '[1]',
          blockId: 'b1',
          start: 2,
          end: 7,
          targets: [{ ordinal: 1, referenceKey: 'ref-1', citedLocator: null }],
        },
      ],
    }

    await expect(transport.syncManuscriptCitations('case/1', 'source/1', input)).resolves.toEqual(
      run,
    )
    await expect(transport.manuscriptCitationSync('sync/run-1')).resolves.toEqual(run)
    await expect(transport.latestManuscriptCitationSync('case/1', 'source/1')).resolves.toEqual(run)
    await expect(transport.manuscriptCitationSyncOccurrences('sync/run-1')).resolves.toEqual([
      occurrence,
    ])
    await expect(transport.manuscriptCitationSyncTargets('sync/occurrence-1')).resolves.toEqual([
      target,
    ])

    expect(requests[0]).toEqual({
      input:
        'http://127.0.0.1:39761/api/research/cases/case%2F1/manuscripts/source%2F1/citations/sync',
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        'x-nineprofs-session-secret': 'research-secret',
      },
      body: JSON.stringify(input),
    })
    expect(requests.slice(1).map(({ input: path }) => path)).toEqual([
      'http://127.0.0.1:39761/api/research/manuscript-citation-sync-runs/sync%2Frun-1',
      'http://127.0.0.1:39761/api/research/cases/case%2F1/manuscripts/source%2F1/citations/sync/latest',
      'http://127.0.0.1:39761/api/research/manuscript-citation-sync-runs/sync%2Frun-1/occurrences',
      'http://127.0.0.1:39761/api/research/manuscript-citation-sync-occurrences/sync%2Foccurrence-1/targets',
    ])
  })

  it('maps manuscript reference catalog sync and read routes through the trusted transport', async () => {
    const requests: Array<{
      input: string
      method?: string
      headers?: Record<string, string>
      body?: string
    }> = []
    const catalogRun = {
      catalogRunId: 'catalog/run-1',
      researchCaseId: 'case-1',
      manuscriptSourceId: 'source-1',
      citationSyncRunId: 'sync/run-1',
      documentId: 'doc-1',
      documentVersion: 4,
      catalogHash: { algorithm: 'sha256', value: 'catalog-hash' },
      entryCount: 1,
      targetMappingCount: 1,
      status: 'completed',
      createdAtMs: 1,
      completedAtMs: 2,
      failureCode: null,
    }
    const entry = {
      entryId: 'entry-1',
      catalogRunId: 'catalog/run-1',
      ordinal: 0,
      format: 'zotero',
      referenceKey: '12',
      descriptorHash: { algorithm: 'sha256', value: 'descriptor-hash' },
      wordSource: null,
      zotero: { itemId: '12', uris: [] },
      targetCount: 1,
    }
    const mapping = {
      mappingId: 'mapping-1',
      catalogRunId: 'catalog/run-1',
      referenceEntryId: 'entry-1',
      citationOccurrenceId: 'citation-occurrence-1',
      citationTargetId: 'citation-target-1',
      documentTargetOrdinal: 1,
    }
    const transport = createCoreTransport(
      'http://127.0.0.1:39761/',
      async (input, init) => {
        requests.push({ input, method: init?.method, headers: init?.headers, body: init?.body })
        const data = input.includes('/entries')
          ? [entry]
          : input.includes('/mappings')
            ? [mapping]
            : catalogRun
        return { ok: true, json: async () => ({ success: true, data }) }
      },
      { sessionSecret: 'research-secret' },
    )
    const input = {
      documentId: 'doc-1',
      documentVersion: 4,
      citations: [
        {
          citationOccurrenceId: 'citation-occurrence-1',
          blockId: 'b1',
          start: 2,
          end: 7,
          format: 'zotero' as const,
          targets: [
            {
              citationTargetId: 'citation-target-1',
              ordinal: 1,
              referenceKey: '12',
              zotero: { itemId: '12', uris: [] },
            },
          ],
        },
      ],
    }

    await expect(transport.syncManuscriptReferenceCatalog('sync/run 1', input)).resolves.toEqual(
      catalogRun,
    )
    await expect(transport.manuscriptReferenceCatalog('sync/run 1')).resolves.toEqual(catalogRun)
    await expect(transport.latestManuscriptReferenceCatalog('case/1', 'source/1')).resolves.toEqual(
      catalogRun,
    )
    await expect(transport.manuscriptReferenceCatalogRun('catalog/run-1')).resolves.toEqual(
      catalogRun,
    )
    await expect(transport.manuscriptReferenceEntries('catalog/run-1')).resolves.toEqual([entry])
    await expect(transport.manuscriptReferenceTargetMappings('entry-1')).resolves.toEqual([mapping])

    expect(requests[0]).toEqual({
      input:
        'http://127.0.0.1:39761/api/research/manuscript-citation-syncs/sync%2Frun%201/reference-catalog',
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        'x-nineprofs-session-secret': 'research-secret',
      },
      body: JSON.stringify(input),
    })
    expect(requests.slice(1).map(({ input: path }) => path)).toEqual([
      'http://127.0.0.1:39761/api/research/manuscript-citation-syncs/sync%2Frun%201/reference-catalog',
      'http://127.0.0.1:39761/api/research/cases/case%2F1/manuscripts/source%2F1/reference-catalog/latest',
      'http://127.0.0.1:39761/api/research/manuscript-reference-catalog-runs/catalog%2Frun-1',
      'http://127.0.0.1:39761/api/research/manuscript-reference-catalog-runs/catalog%2Frun-1/entries',
      'http://127.0.0.1:39761/api/research/manuscript-reference-entries/entry-1/mappings',
    ])
  })

  it('maps citation verification runs and keeps creation on the trusted boundary', async () => {
    const requests: Array<{
      input: string
      method?: string
      headers?: Record<string, string>
      body?: string
    }> = []
    const run = {
      runId: 'citation-verification-1',
      status: 'completed',
      candidates: [],
      evidence: [],
    }
    const transport = createCoreTransport(
      'http://127.0.0.1:39761/',
      async (input, init) => {
        requests.push({ input, method: init?.method, headers: init?.headers, body: init?.body })
        return {
          ok: true,
          json: async () => ({
            success: true,
            data: input.includes('/api/research/claims/') ? [run] : run,
          }),
        }
      },
      { sessionSecret: 'research-secret' },
    )

    await expect(
      transport.createCitationVerification({
        claimCitationLinkId: 'claim-citation-1',
        citationTargetBindingId: 'binding-1',
      }),
    ).resolves.toEqual(run)
    await expect(transport.citationVerification('run/1')).resolves.toEqual(run)
    await expect(transport.claimCitationVerifications('claim/1')).resolves.toEqual([run])

    expect(requests).toEqual([
      {
        input: 'http://127.0.0.1:39761/api/research/citation-verifications',
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          'x-nineprofs-session-secret': 'research-secret',
        },
        body: '{"claimCitationLinkId":"claim-citation-1","citationTargetBindingId":"binding-1"}',
      },
      {
        input: 'http://127.0.0.1:39761/api/research/citation-verifications/run%2F1',
        method: undefined,
        headers: undefined,
        body: undefined,
      },
      {
        input: 'http://127.0.0.1:39761/api/research/claims/claim%2F1/citation-verifications',
        method: undefined,
        headers: undefined,
        body: undefined,
      },
    ])
  })

  it('maps whole-manuscript research review orchestration and read models', async () => {
    const requests: Array<{ input: string; method?: string; body?: string }> = []
    const transport = createCoreTransport('http://127.0.0.1:39761', async (input, init) => {
      requests.push({
        input,
        method: init?.method,
        body: typeof init?.body === 'string' ? init.body : undefined,
      })
      return { ok: true, json: async () => ({ success: true, data: {} }) }
    })

    await transport.startManuscriptResearchReview('case/1', {
      manuscriptSourceId: 'source/1',
      documentId: 'doc-1',
      documentVersion: 4,
      citationReviewObservations: { citations: [], citationBlocks: [] },
      claimInventoryObservations: { wholeManuscriptBlocks: [] },
    })
    await transport.manuscriptResearchReview('review/1')
    await transport.manuscriptResearchReviewClaims('review/1')
    await transport.manuscriptResearchReviewConsistency('review/1')

    expect(requests).toEqual([
      {
        input: 'http://127.0.0.1:39761/api/research/cases/case%2F1/manuscript-research-reviews',
        method: 'POST',
        body: JSON.stringify({
          manuscriptSourceId: 'source/1',
          documentId: 'doc-1',
          documentVersion: 4,
          citationReviewObservations: { citations: [], citationBlocks: [] },
          claimInventoryObservations: { wholeManuscriptBlocks: [] },
        }),
      },
      {
        input: 'http://127.0.0.1:39761/api/research/manuscript-research-reviews/review%2F1',
      },
      {
        input: 'http://127.0.0.1:39761/api/research/manuscript-research-reviews/review%2F1/claims',
      },
      {
        input:
          'http://127.0.0.1:39761/api/research/manuscript-research-reviews/review%2F1/consistency',
      },
    ])
  })
})
