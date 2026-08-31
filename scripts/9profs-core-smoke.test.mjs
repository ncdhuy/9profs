import { strict as assert } from 'node:assert'
import { spawn } from 'node:child_process'
import { createServer } from 'node:net'
import { once } from 'node:events'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'

import { MODEL_TASKS, probeCore, terminateChild, waitForCore } from './9profs.mjs'

async function freePort() {
  const server = createServer()
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  const port = server.address().port
  server.close()
  await once(server, 'close')
  return port
}

async function removeTemporaryDirectory(path) {
  await new Promise((resolve) => setTimeout(resolve, 500))
  let lastError
  for (let attempt = 0; attempt < 5; attempt += 1) {
    try {
      rmSync(path, { recursive: true, force: true, maxRetries: 10, retryDelay: 250 })
      return
    } catch (error) {
      lastError = error
      await new Promise((resolve) => setTimeout(resolve, 500))
    }
  }
  throw lastError
}

test('nineprofs-core boots with temporary storage and exposes health/runtime', async () => {
  const dataDir = mkdtempSync(join(tmpdir(), '9profs-core-smoke-'))
  const port = await freePort()
  const env = {
    ...process.env,
    NINEPROFS_CORE_ADDR: `127.0.0.1:${port}`,
    NINEPROFS_CORE_DATA_DIR: dataDir,
  }
  for (const task of MODEL_TASKS) {
    for (const name of [
      task.providerEnv,
      task.modelEnv,
      task.baseUrlEnv,
      task.apiKeyEnvEnv,
      task.timeoutEnv,
    ])
      delete env[name]
  }
  for (const name of [
    'NINEPROFS_CORE_URL',
    'NINEPROFS_DIFY_BASE_URL',
    'NINEPROFS_DIFY_API_KEY',
    'NINEPROFS_AGENT_PROVIDER',
    'NINEPROFS_AGENT_MODEL',
    'NINEPROFS_AGENT_BASE_URL',
    'NINEPROFS_AGENT_API_KEY_ENV',
    'OPENAI_API_KEY',
    'ANTHROPIC_API_KEY',
  ])
    delete env[name]

  const child = spawn(
    process.platform === 'win32' ? 'cargo.exe' : 'cargo',
    ['run', '--manifest-path', '9profs-core-rs/Cargo.toml', '--locked', '--bin', 'nineprofs-core'],
    { cwd: new URL('..', import.meta.url), env, stdio: 'ignore', windowsHide: true },
  )
  try {
    const result = await waitForCore(`http://127.0.0.1:${port}`, {
      child,
      timeoutMs: 180_000,
      pollIntervalMs: 250,
      probe: (url) => probeCore(url, { timeoutMs: 1_000 }),
    })
    assert.equal(result.compatible, true)
    const health = await probeCore(`http://127.0.0.1:${port}`)
    assert.equal(health.compatible, true)
  } finally {
    terminateChild(child)
    if (child.exitCode === null) {
      await Promise.race([
        once(child, 'exit'),
        new Promise((resolve) => setTimeout(resolve, 5_000)),
      ])
    }
    await removeTemporaryDirectory(dataDir)
  }
})
