/**
 * Transport-neutral DTO mapping for the optional 9Profs Core HTTP boundary.
 * Rust remains an implementation detail; callers depend only on these values.
 */
export interface CoreResponse<T> {
  success: boolean
  data?: T
  message?: string
}

export interface CoreHealth {
  status: 'ok'
  service: '9profs-core'
}

export interface CoreRuntimeInfo {
  service: '9profs-core'
  version: string
  protocol_version: string
  capabilities: string[]
}

export type CoreFetch = (input: string) => Promise<{
  ok: boolean
  json(): Promise<unknown>
}>

export interface CoreTransport {
  health(): Promise<CoreHealth>
  runtime(): Promise<CoreRuntimeInfo>
  websocketUrl(): string
}

export function createCoreTransport(baseUrl: string, fetcher: CoreFetch): CoreTransport {
  const normalizedBaseUrl = baseUrl.replace(/\/+$/, '')

  async function get<T>(path: string): Promise<T> {
    const response = await fetcher(`${normalizedBaseUrl}${path}`)
    if (!response.ok) throw new Error(`9Profs Core request failed: ${path}`)

    const body = (await response.json()) as CoreResponse<T>
    if (!body.success || body.data === undefined)
      throw new Error(`9Profs Core response failed: ${path}`)
    return body.data
  }

  return {
    health: () => get<CoreHealth>('/api/health'),
    runtime: () => get<CoreRuntimeInfo>('/api/runtime'),
    websocketUrl: () => normalizedBaseUrl.replace(/^http/, 'ws') + '/ws',
  }
}
