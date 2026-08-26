import { describe, expect, it } from 'vitest'
import { extractResearchPdfPages } from '../src/research-pdf'
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
    await transport.researchPdfExtraction('snapshot/1')
    await transport.researchPdfPages('extraction/1', 25)
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
    expect(requests[5].init?.body).not.toContain('Evidence')
    expect(requests[5].init?.body).toContain('"start":0')
    await expect(extractResearchPdfPages(new Uint8Array([1, 2, 3]))).resolves.toMatchObject({
      status: 'failed',
      pages: [],
    })
  })
})
