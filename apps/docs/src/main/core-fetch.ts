import type { CoreFetchResponse } from '../shared/ipc'

export const MAX_CORE_RESPONSE_BYTES = 4 * 1024 * 1024

interface CoreNetworkRequest {
  method: string
  headers?: Record<string, string>
  body?: string | Uint8Array
  redirect: 'error'
}

interface CoreNetworkResponse {
  ok: boolean
  status: number
  text(): Promise<string>
}

export type CoreNetworkFetch = (
  url: string,
  init: CoreNetworkRequest,
) => Promise<CoreNetworkResponse>

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function rejected(): never {
  throw new Error('Core request rejected')
}

function normalizeRequest(request: unknown, baseUrl: string): { url: string; init: CoreNetworkRequest } {
  if (!isRecord(request) || typeof request.url !== 'string') rejected()

  let base: URL
  let target: URL
  try {
    base = new URL(baseUrl)
    target = new URL(request.url, base)
  } catch {
    rejected()
  }
  if ((base.protocol !== 'http:' && base.protocol !== 'https:') || target.origin !== base.origin) {
    rejected()
  }

  const method = request.method === undefined ? 'GET' : request.method
  if (typeof method !== 'string' || !['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].includes(method.toUpperCase())) {
    rejected()
  }

  let headers: Record<string, string> | undefined
  if (request.headers !== undefined) {
    if (!isRecord(request.headers)) rejected()
    headers = {}
    for (const [name, value] of Object.entries(request.headers)) {
      if (typeof value !== 'string') rejected()
      headers[name] = value
    }
  }

  const body = request.body
  if (body !== undefined && typeof body !== 'string' && !(body instanceof Uint8Array)) rejected()

  return {
    url: target.toString(),
    init: {
      method: method.toUpperCase(),
      headers,
      body: body instanceof Uint8Array ? new Uint8Array(body) : body,
      redirect: 'error',
    },
  }
}

function responseBody(text: string): unknown {
  const bytes = new TextEncoder().encode(text)
  if (bytes.byteLength > MAX_CORE_RESPONSE_BYTES) {
    return new TextDecoder().decode(bytes.slice(0, MAX_CORE_RESPONSE_BYTES))
  }
  try {
    return JSON.parse(text) as unknown
  } catch {
    return text
  }
}

export async function performCoreFetch(
  request: unknown,
  baseUrl: string,
  fetcher: CoreNetworkFetch,
): Promise<CoreFetchResponse> {
  const normalized = normalizeRequest(request, baseUrl)
  try {
    const response = await fetcher(normalized.url, normalized.init)
    return {
      ok: response.ok,
      status: response.status,
      json: responseBody(await response.text()),
    }
  } catch {
    throw new Error('Core request failed')
  }
}
