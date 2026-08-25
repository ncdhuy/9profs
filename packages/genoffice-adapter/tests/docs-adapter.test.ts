import { describe, expect, it } from 'vitest'
import {
  createGenOfficeDocsAdapter,
  DOCS_COMMAND_ENVELOPE,
  GenOfficeDocsMutationError,
  type DocsCommandContext,
  type GenOfficeDocsRuntime,
} from '../src'

function approved(
  documentId: string,
  baseVersion: number,
  changes: readonly { type: string; payload?: Readonly<Record<string, unknown>> }[] = [
    {
      type: DOCS_COMMAND_ENVELOPE,
      payload: { commands: [{ replaceAllText: { containsText: 'old', replaceText: 'new' } }] },
    },
  ],
) {
  return {
    id: 'change-set-1',
    status: 'approved' as const,
    target: { kind: 'genoffice-active' as const, documentId, writeAuthority: 'genoffice' as const },
    baseVersion,
    changes,
    approval: { approvedBy: 'reviewer', approvedAt: '2026-08-26T00:00:00Z' },
  }
}

function harness(initialVersion = 0) {
  const listeners = new Set<(transaction: { docChanged: boolean }) => void>()
  const calls: Array<{ commands: readonly unknown[]; context: DocsCommandContext }> = []
  let content = 'old'
  let nextOutcome: ReturnType<GenOfficeDocsRuntime['executeCommands']> = {
    ok: true,
    results: [{ changed: 1 }],
    summary: 'changed',
  }
  const runtime: GenOfficeDocsRuntime = {
    subscribeToTransactions(listener) {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    buildDocumentContext: () => `content:${content}`,
    getSelectionContext: () => ({ from: 1, to: 1, empty: true }),
    executeCommands(commands, context) {
      calls.push({ commands, context })
      const changed = nextOutcome.results.some((result) => (result.changed ?? 0) > 0)
      if (nextOutcome.ok && changed) {
        content = 'new'
        for (const listener of listeners) listener({ docChanged: true })
      }
      return nextOutcome
    },
  }
  const adapter = createGenOfficeDocsAdapter({
    documentId: 'doc-1',
    runtime,
    initialVersion,
  })
  return {
    adapter,
    calls,
    get content() {
      return content
    },
    emit(docChanged: boolean) {
      for (const listener of listeners) listener({ docChanged })
    },
    setOutcome(outcome: ReturnType<GenOfficeDocsRuntime['executeCommands']>) {
      nextOutcome = outcome
    },
  }
}

describe('GenOffice active Docs adapter', () => {
  it('inspects active context and tracks document transactions', async () => {
    const h = harness()
    const inspection = await h.adapter.inspector.inspect({ documentId: 'doc-1' })
    expect(inspection).toMatchObject({
      documentId: 'doc-1',
      authority: { kind: 'genoffice-active', documentId: 'doc-1' },
      version: 0,
      value: { context: 'content:old', selection: { from: 1, to: 1, empty: true } },
    })

    h.emit(false)
    expect((await h.adapter.inspector.inspect({ documentId: 'doc-1' })).version).toBe(0)
    h.emit(true)
    expect((await h.adapter.inspector.inspect({ documentId: 'doc-1' })).version).toBe(1)
    h.adapter.dispose()
  })

  it('combines multiple command changes into one engine call and returns observed version', async () => {
    const h = harness()
    const result = await h.adapter.mutationGateway.commit(
      approved('doc-1', 0, [
        { type: DOCS_COMMAND_ENVELOPE, payload: { commands: [{ first: {} }] } },
        { type: DOCS_COMMAND_ENVELOPE, payload: { commands: [{ second: {} }] } },
      ]),
    )
    expect(result).toMatchObject({
      status: 'applied',
      previousVersion: 0,
      newVersion: 1,
      commandCount: 2,
      changedCount: 1,
    })
    expect(h.calls).toHaveLength(1)
    expect(h.calls[0].commands).toEqual([{ first: {} }, { second: {} }])
    expect(h.calls[0].context).toEqual({ track: { author: '9Profs AI' } })
    h.adapter.dispose()
  })

  it('rejects stale changes without invoking the command engine', async () => {
    const h = harness()
    h.emit(true)
    const result = await h.adapter.mutationGateway.commit(approved('doc-1', 0))
    expect(result).toEqual({
      changeSetId: 'change-set-1',
      documentId: 'doc-1',
      status: 'conflict',
      reason: 'stale-version',
      baseVersion: 0,
      currentVersion: 1,
    })
    expect(h.calls).toHaveLength(0)
    expect(h.content).toBe('old')
    h.adapter.dispose()
  })

  it('rejects identity and unsupported changes before dispatch', async () => {
    const h = harness()
    await expect(h.adapter.mutationGateway.commit(approved('other-doc', 0))).rejects.toMatchObject({
      code: 'document-mismatch',
    })
    await expect(
      h.adapter.mutationGateway.commit(
        approved('doc-1', 0, [{ type: 'office.mutate_detached', payload: {} }]),
      ),
    ).rejects.toMatchObject({ code: 'unsupported-change' })
    await expect(
      h.adapter.mutationGateway.commit({
        ...approved('doc-1', 0),
        status: 'proposed',
      } as never),
    ).rejects.toMatchObject({ code: 'invalid-status' })
    await expect(
      h.adapter.mutationGateway.commit(
        approved('doc-1', 0, [
          { type: DOCS_COMMAND_ENVELOPE, payload: { commands: 'not-an-array' as never } },
        ]),
      ),
    ).rejects.toMatchObject({ code: 'invalid-change-payload' })
    expect(h.calls).toHaveLength(0)
    h.adapter.dispose()
  })

  it('keeps invalid envelopes atomic and reports no-op without version change', async () => {
    const h = harness()
    h.setOutcome({ ok: false, results: [], summary: '', error: 'command #0: unknown command' })
    await expect(h.adapter.mutationGateway.commit(approved('doc-1', 0))).rejects.toMatchObject({
      code: 'invalid-command-envelope',
    })
    expect(h.content).toBe('old')
    expect((await h.adapter.inspector.inspect({ documentId: 'doc-1' })).version).toBe(0)

    h.setOutcome({ ok: true, results: [{ changed: 0 }], summary: 'no changes' })
    const result = await h.adapter.mutationGateway.commit(approved('doc-1', 0))
    expect(result).toMatchObject({ status: 'applied', previousVersion: 0, newVersion: 0 })
    h.adapter.dispose()
  })

  it('supports clean version-session reset and typed inspection identity rejection', async () => {
    const h = harness(5)
    expect((await h.adapter.inspector.inspect({ documentId: 'doc-1' })).version).toBe(5)
    h.adapter.resetVersion()
    expect((await h.adapter.inspector.inspect({ documentId: 'doc-1' })).version).toBe(0)
    await expect(h.adapter.inspector.inspect({ documentId: 'other-doc' })).rejects.toBeInstanceOf(
      GenOfficeDocsMutationError,
    )
    h.adapter.dispose()
  })
})
