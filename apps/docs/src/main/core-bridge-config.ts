export function coreWebsocketUrlFromHttpBaseUrl(baseUrl: string): string {
  const url = new URL(baseUrl)
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:'
  url.pathname = '/ws/documents'
  url.search = ''
  url.hash = ''
  return url.toString()
}
