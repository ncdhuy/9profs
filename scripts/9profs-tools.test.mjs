import { strict as assert } from 'node:assert'
import { EventEmitter } from 'node:events'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'

import {
  MODEL_TASKS,
  buildNpmCommand,
  createDoctorReport,
  ensureCore,
  evaluateModelReadiness,
  formatDoctorReport,
  loadDogfoodingEnv,
  parseEnvFile,
  setupLocalEnv,
  superviseChildren,
  waitForCore,
} from './9profs.mjs'

const tempDirectory = () => mkdtempSync(join(tmpdir(), '9profs-tools-'))

function healthyCoreProbe() {
  return { reachable: true, compatible: true, reason: '' }
}

test('setup creates local env and never overwrites it', () => {
  const rootDir = tempDirectory()
  try {
    writeFileSync(join(rootDir, '.env.9profs.example'), 'NINEPROFS_CORE_ADDR=127.0.0.1:39761\n')
    const first = setupLocalEnv(rootDir)
    assert.equal(first.created, true)
    assert.equal(
      readFileSync(join(rootDir, '.env.9profs'), 'utf8'),
      'NINEPROFS_CORE_ADDR=127.0.0.1:39761\n',
    )

    writeFileSync(join(rootDir, '.env.9profs'), 'LOCAL_EDIT=preserve\n')
    const second = setupLocalEnv(rootDir)
    assert.equal(second.created, false)
    assert.equal(readFileSync(join(rootDir, '.env.9profs'), 'utf8'), 'LOCAL_EDIT=preserve\n')
  } finally {
    rmSync(rootDir, { recursive: true, force: true })
  }
})

test('example contains runtime Research configuration without secrets', () => {
  const example = readFileSync(new URL('../.env.9profs.example', import.meta.url), 'utf8')
  const parsed = parseEnvFile(example)
  const requiredNames = [
    'NINEPROFS_CORE_ADDR',
    'NINEPROFS_CORE_DATA_DIR',
    ...MODEL_TASKS.flatMap((task) => [
      task.providerEnv,
      task.modelEnv,
      task.baseUrlEnv,
      task.apiKeyEnvEnv,
    ]),
    'NINEPROFS_DIFY_BASE_URL',
    'NINEPROFS_DIFY_API_KEY',
    'NINEPROFS_DIFY_TIMEOUT_MS',
    'NINEPROFS_DIFY_INDEXING_TECHNIQUE',
  ]
  for (const name of requiredNames) assert.match(example, new RegExp(`^${name}=`, 'm'))
  assert.equal(parsed.OPENAI_API_KEY, '')
  assert.equal(parsed.NINEPROFS_DIFY_API_KEY, '')
  assert.doesNotMatch(example, /sk-[A-Za-z0-9]{10,}/)
})

test('doctor distinguishes provider readiness states', () => {
  const task = MODEL_TASKS[0]
  const missing = evaluateModelReadiness({}, task)
  assert.equal(missing.status, 'NOT CONFIGURED')
  assert.match(missing.reason, /NINEPROFS_CLAIM_EXTRACTOR_PROVIDER/)

  const ready = evaluateModelReadiness(
    {
      [task.providerEnv]: 'openai',
      [task.modelEnv]: 'local-model',
      [task.baseUrlEnv]: 'http://127.0.0.1:1234/v1',
      [task.apiKeyEnvEnv]: 'LOCAL_MODEL_KEY',
      LOCAL_MODEL_KEY: 'test-value',
    },
    task,
  )
  assert.equal(ready.status, 'READY')
})

test('doctor reports healthy Core, missing providers, and missing Dify without secrets', async () => {
  const report = await createDoctorReport({
    baseEnv: { NINEPROFS_CORE_ADDR: '127.0.0.1:39761' },
    probe: healthyCoreProbe,
    fetchImpl: async () => {
      throw new Error('Dify should not be called when unconfigured')
    },
  })
  const output = formatDoctorReport(report)
  assert.equal(report.exitCode, 0)
  assert.match(output, /Core configuration\s+OK/)
  assert.match(output, /Core reachable\s+OK/)
  assert.match(output, /Claim extractor\s+NOT CONFIGURED/)
  assert.match(output, /Dify\s+NOT CONFIGURED/)
  assert.match(output, /Citation verification unavailable/)
  assert.doesNotMatch(output, /test-value|OPENAI_API_KEY=/)
})

test('doctor reports unavailable Core and does not treat it as ready', async () => {
  const report = await createDoctorReport({
    baseEnv: { NINEPROFS_CORE_ADDR: '127.0.0.1:39761' },
    probe: async () => ({ reachable: false, compatible: false, reason: 'Core did not answer' }),
    portOpen: async () => false,
  })
  const output = formatDoctorReport(report)
  assert.equal(report.exitCode, 1)
  assert.match(output, /Core reachable\s+NOT REACHABLE/)
  assert.match(output, /Start with npm run dev:9profs/)
})

test('doctor uses retrieval readiness API for configured Dify', async () => {
  const report = await createDoctorReport({
    baseEnv: {
      NINEPROFS_CORE_ADDR: '127.0.0.1:39761',
      NINEPROFS_DIFY_BASE_URL: 'http://127.0.0.1:5001/v1',
      NINEPROFS_DIFY_API_KEY: 'test-value',
    },
    probe: healthyCoreProbe,
    fetchImpl: async (url) => {
      assert.match(url, /\/api\/research\/cases\/__9profs_doctor__\/retrieval-index$/)
      return new Response(
        JSON.stringify({
          success: true,
          data: { readiness: { status: 'ready', ready: true } },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      )
    },
  })
  assert.equal(report.dify.status, 'READY')
})

test('dev launcher waits for Core and builds cross-platform npm commands', async () => {
  const env = { NINEPROFS_CORE_ADDR: '127.0.0.1:39761' }
  const probes = [
    { reachable: false, compatible: false, reason: 'Core did not answer' },
    { reachable: true, compatible: true, reason: '' },
  ]
  const child = new EventEmitter()
  child.exitCode = null
  const launches = []
  const result = await ensureCore({
    env,
    probe: async () => probes.shift(),
    portOpen: async () => false,
    spawnImpl: (command, args, options) => {
      launches.push({ command, args, options })
      return child
    },
    sleepImpl: async () => {},
    log: () => {},
  })
  assert.equal(result.started, true)
  assert.deepEqual(launches[0].args, ['run', 'core:run'])
  assert.equal(buildNpmCommand('dev').args.join(' '), 'run dev')
})

test('dev launcher rejects an occupied non-Core port', async () => {
  let spawned = false
  await assert.rejects(
    ensureCore({
      env: { NINEPROFS_CORE_ADDR: '127.0.0.1:39761' },
      probe: async () => ({ reachable: false, compatible: false, reason: 'Core did not answer' }),
      portOpen: async () => true,
      spawnImpl: () => {
        spawned = true
        throw new Error('must not spawn')
      },
    }),
    /occupied.*not a compatible 9Profs Core/,
  )
  assert.equal(spawned, false)
})

test('Core readiness wait retries unreachable probes and stops at compatible Core', async () => {
  let calls = 0
  const result = await waitForCore('http://127.0.0.1:39761', {
    probe: async () => {
      calls += 1
      return calls === 1
        ? { reachable: false, compatible: false, reason: 'Core did not answer' }
        : { reachable: true, compatible: true, reason: '' }
    },
    timeoutMs: 100,
    pollIntervalMs: 1,
    sleepImpl: async () => {},
  })
  assert.equal(result.compatible, true)
  assert.equal(calls, 2)
})

test('supervisor terminates owned Core when app exits', async () => {
  const app = new EventEmitter()
  const core = new EventEmitter()
  app.exitCode = null
  core.exitCode = null
  const terminated = []
  const resultPromise = superviseChildren(app, core, {
    terminate: (child) => terminated.push(child),
  })
  app.emit('exit', 0, null)
  assert.equal(await resultPromise, 0)
  assert.deepEqual(terminated, [core])
})

test('environment file values fill missing process values without overriding them', () => {
  const rootDir = tempDirectory()
  try {
    writeFileSync(join(rootDir, '.env.9profs'), 'FROM_FILE=file-value\nKEPT=file-value\n')
    const loaded = loadDogfoodingEnv(rootDir, { KEPT: 'process-value' })
    assert.equal(loaded.env.FROM_FILE, 'file-value')
    assert.equal(loaded.env.KEPT, 'process-value')
  } finally {
    rmSync(rootDir, { recursive: true, force: true })
  }
})
