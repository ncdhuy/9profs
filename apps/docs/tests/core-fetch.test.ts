import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import { createCoreTransport } from '@genoffice/9profs-core'
import type { CoreFetchRequest } from '../src/shared/ipc'
import { performCoreFetch, type CoreNetworkFetch } from '../src/main/core-fetch'

const baseUrl = 'http://127.0.0.1:39761'

function response(data: unknown) {
  return { ok: true, status: 200, text: async () => JSON.stringify({ success: true, data }) }
}

function bridgeFetcher(network: CoreNetworkFetch) {
  return async (url: string, init?: { method?: string; headers?: Record<string, string>; body?: string | Uint8Array }) => {
    const request: CoreFetchRequest = {
      url,
      method: init?.method,
      headers: init?.headers,
      body: init?.body,
    }
    const result = await performCoreFetch(request, baseUrl, network)
    return { ok: result.ok, json: async () => result.json }
  }
}

describe('Docs Electron Core fetch bridge', () => {
  it('loads empty ResearchCase list as successful CoreTransport data', async () => {
    const network = vi.fn<CoreNetworkFetch>(async () => response([]))
    const transport = createCoreTransport(baseUrl, bridgeFetcher(network))

    await expect(transport.researchCases()).resolves.toEqual([])
    expect(network).toHaveBeenCalledWith(
      `${baseUrl}/api/research/cases`,
      expect.objectContaining({ method: 'GET', redirect: 'error' }),
    )
  })

  it('forwards trusted JSON POST headers and body', async () => {
    const network = vi.fn<CoreNetworkFetch>(async () => response({ caseId: 'case-1' }))
    const transport = createCoreTransport(
      baseUrl,
      bridgeFetcher(network),
      { sessionSecret: 'test-session-secret' },
    )

    await transport.createResearchCase({ title: 'Review' })

    expect(network).toHaveBeenCalledWith(
      `${baseUrl}/api/research/cases`,
      expect.objectContaining({
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          'x-nineprofs-session-secret': 'test-session-secret',
        },
        body: JSON.stringify({ title: 'Review' }),
        redirect: 'error',
      }),
    )
  })

  it('forwards binary request bytes unchanged', async () => {
    const bytes = new Uint8Array([37, 80, 68, 70, 0, 255])
    let forwarded: Uint8Array | undefined
    const network = vi.fn<CoreNetworkFetch>(async (_url, init) => {
      forwarded = init.body instanceof Uint8Array ? init.body : undefined
      return response({})
    })

    await performCoreFetch(
      { url: '/api/research/cases/case-1/reference-pdfs', method: 'POST', body: bytes },
      baseUrl,
      network,
    )

    expect(forwarded).toBeInstanceOf(Uint8Array)
    expect(Array.from(forwarded ?? [])).toEqual(Array.from(bytes))
  })

  it('rejects non-Core destinations and keeps secrets out of failures', async () => {
    const secret = 'do-not-expose-this-secret'
    const network = vi.fn<CoreNetworkFetch>(async () => response([]))

    await expect(
      performCoreFetch(
        { url: 'https://example.com/steal', headers: { 'x-nineprofs-session-secret': secret } },
        baseUrl,
        network,
      ),
    ).rejects.toThrow('Core request rejected')
    expect(network).not.toHaveBeenCalled()

    network.mockRejectedValueOnce(new Error(`network failed: ${secret}`))
    const failure = performCoreFetch({ url: '/api/health' }, baseUrl, network)
    await expect(failure).rejects.toThrow('Core request failed')
    await failure.catch((error: unknown) => expect(String(error)).not.toContain(secret))
  })

  it('does not use renderer global fetch for CoreTransport construction', () => {
    const source = readFileSync(join(__dirname, '../src/renderer/App.tsx'), 'utf8')
    const preload = readFileSync(join(__dirname, '../src/preload/index.ts'), 'utf8')
    expect(source).toContain('window.desktop.coreFetch')
    expect(source).not.toMatch(/window\.fetch\s*\(/)
    expect(preload).toContain("ipcRenderer.invoke('docs:core-fetch', request)")
  })
})
