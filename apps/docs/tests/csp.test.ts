import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

const docsHtml = readFileSync(join(__dirname, '../src/renderer/index.html'), 'utf8')
const docsMain = readFileSync(join(__dirname, '../src/main/docs-main.ts'), 'utf8')
const connectSources =
  docsHtml
    .match(/<meta\s+http-equiv="Content-Security-Policy"\s+content="([^"]+)"/i)?.[1]
    ?.match(/(?:^|;)\s*connect-src\s+([^;]+)/i)?.[1]
    ?.trim()
    .split(/\s+/) ?? []

function allowsWebSocket(url: string): boolean {
  const candidate = new URL(url)
  return connectSources.some((source) => {
    const match = source.match(/^(ws|wss):\/\/([^:]+):\*$/)
    return match?.[1] === candidate.protocol.slice(0, -1) && match[2] === candidate.hostname
  })
}

describe('Docs Content Security Policy', () => {
  it('allows trusted local Core WebSocket origins on any local port', () => {
    expect(allowsWebSocket('ws://localhost:39761')).toBe(true)
    expect(allowsWebSocket('ws://127.0.0.1:39761')).toBe(true)
  })

  it('does not broaden WebSocket access beyond the trusted loopback hosts', () => {
    expect(allowsWebSocket('ws://example.com')).toBe(false)
    expect(allowsWebSocket('ws://192.168.1.10')).toBe(false)
    expect(allowsWebSocket('ws://example.com:39761')).toBe(false)
    expect(allowsWebSocket('ws://192.168.1.10:39761')).toBe(false)
    expect(allowsWebSocket('ws://anything:39761')).toBe(false)
    expect(connectSources).not.toContain('ws://*')
    expect(connectSources).not.toContain('*')
  })
})

describe('Docs Electron security settings', () => {
  it('keeps renderer isolation, sandbox, and web security enabled', () => {
    expect(docsMain).toMatch(/contextIsolation:\s*true/)
    expect(docsMain).toMatch(/nodeIntegration:\s*false/)
    expect(docsMain).toMatch(/sandbox:\s*true/)
    expect(docsMain).toMatch(/webSecurity:\s*true/)
    expect(docsMain).not.toMatch(/webSecurity:\s*false/)
  })
})
