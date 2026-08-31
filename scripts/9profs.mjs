import { execFileSync, spawn as defaultSpawn } from 'node:child_process'
import { copyFileSync, existsSync, readFileSync } from 'node:fs'
import { createConnection } from 'node:net'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const ROOT_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '..')
export const LOCAL_ENV_FILENAME = '.env.9profs'
export const EXAMPLE_ENV_FILENAME = '.env.9profs.example'
export const DEFAULT_CORE_ADDR = '127.0.0.1:39761'
const DOCTOR_RETRIEVAL_CASE_ID = '__9profs_doctor__'
const DEFAULT_POLL_INTERVAL_MS = 250
const DEFAULT_CORE_TIMEOUT_MS = 180_000

export const SEMANTIC_MODEL_ENV = {
  providerEnv: 'NINEPROFS_MODEL_PROVIDER',
  modelEnv: 'NINEPROFS_MODEL_MODEL',
  baseUrlEnv: 'NINEPROFS_MODEL_BASE_URL',
  apiKeyEnvEnv: 'NINEPROFS_MODEL_API_KEY_ENV',
  timeoutEnv: 'NINEPROFS_MODEL_TIMEOUT_MS',
}

export const MODEL_TASKS = [
  'Claim extractor',
  'Citation assessor',
  'Citation expectation',
  'Cross-claim discovery',
  'Cross-claim assessment',
  'Regulation requirement candidates',
].map((label) => ({ label, ...SEMANTIC_MODEL_ENV }))

const ENV_ASSIGNMENT = /^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$/

function stripInlineComment(value) {
  let quote = null
  for (let index = 0; index < value.length; index += 1) {
    const character = value[index]
    if ((character === '"' || character === "'") && value[index - 1] !== '\\') {
      quote = quote === character ? null : quote || character
    }
    if (character === '#' && !quote && /\s/.test(value[index - 1] || '')) {
      return value.slice(0, index).trimEnd()
    }
  }
  return value
}

function unquote(value) {
  if (value.length >= 2 && value.startsWith('"') && value.endsWith('"')) {
    try {
      return JSON.parse(value)
    } catch {
      return value.slice(1, -1)
    }
  }
  if (value.length >= 2 && value.startsWith("'") && value.endsWith("'")) {
    return value.slice(1, -1)
  }
  return value
}

export function parseEnvFile(content) {
  const values = {}
  for (const line of content.split(/\r?\n/)) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith('#')) continue
    const assignment = trimmed.replace(/^export\s+/, '').match(ENV_ASSIGNMENT)
    if (!assignment) continue
    values[assignment[1]] = unquote(stripInlineComment(assignment[2].trim()))
  }
  return values
}

export function envPaths(rootDir = ROOT_DIR) {
  return {
    examplePath: join(rootDir, EXAMPLE_ENV_FILENAME),
    localPath: join(rootDir, LOCAL_ENV_FILENAME),
  }
}

export function loadDogfoodingEnv(rootDir = ROOT_DIR, baseEnv = process.env) {
  const { localPath } = envPaths(rootDir)
  const fileValues = existsSync(localPath) ? parseEnvFile(readFileSync(localPath, 'utf8')) : {}
  const env = { ...baseEnv }
  for (const [key, value] of Object.entries(fileValues)) {
    if (env[key] === undefined) env[key] = value
  }
  return { env, fileExists: existsSync(localPath), localPath }
}

export function setupLocalEnv(rootDir = ROOT_DIR) {
  const { examplePath, localPath } = envPaths(rootDir)
  if (!existsSync(examplePath)) throw new Error(`Missing ${EXAMPLE_ENV_FILENAME}`)
  if (existsSync(localPath)) return { created: false, localPath }
  try {
    copyFileSync(examplePath, localPath, 1)
    return { created: true, localPath }
  } catch (error) {
    if (error?.code === 'EEXIST') return { created: false, localPath }
    throw error
  }
}

function valueOf(env, name) {
  return typeof env[name] === 'string' ? env[name].trim() : ''
}

function defaultApiKeyEnv(provider) {
  return provider === 'anthropic' ? 'ANTHROPIC_API_KEY' : 'OPENAI_API_KEY'
}

export function evaluateModelReadiness(env, task) {
  const provider = valueOf(env, SEMANTIC_MODEL_ENV.providerEnv)
  const model = valueOf(env, SEMANTIC_MODEL_ENV.modelEnv)
  const baseUrl = valueOf(env, SEMANTIC_MODEL_ENV.baseUrlEnv)
  const apiKeyEnv = valueOf(env, SEMANTIC_MODEL_ENV.apiKeyEnvEnv) || defaultApiKeyEnv(provider)

  if (!provider && !model && !baseUrl) {
    return {
      status: 'NOT CONFIGURED',
      reason: `Set ${SEMANTIC_MODEL_ENV.providerEnv} and ${SEMANTIC_MODEL_ENV.modelEnv}.`,
    }
  }
  if (!provider) {
    return { status: 'NOT READY', reason: `Set ${SEMANTIC_MODEL_ENV.providerEnv}.` }
  }
  if (provider !== 'openai' && provider !== 'anthropic') {
    return {
      status: 'NOT READY',
      reason: `${SEMANTIC_MODEL_ENV.providerEnv} must be openai or anthropic.`,
    }
  }
  if (!model) return { status: 'NOT READY', reason: `Set ${SEMANTIC_MODEL_ENV.modelEnv}.` }
  if (baseUrl) {
    try {
      const parsed = new URL(baseUrl)
      if (!['http:', 'https:'].includes(parsed.protocol) || !parsed.hostname) throw new Error()
    } catch {
      return {
        status: 'NOT READY',
        reason: `Set ${SEMANTIC_MODEL_ENV.baseUrlEnv} to an http(s) URL or leave it empty.`,
      }
    }
  }
  if (!valueOf(env, apiKeyEnv)) {
    return {
      status: 'NOT READY',
      reason: `Set ${apiKeyEnv} (named by ${SEMANTIC_MODEL_ENV.apiKeyEnvEnv}).`,
    }
  }
  return { status: 'READY', reason: '' }
}

function coreAddress(env) {
  return valueOf(env, 'NINEPROFS_CORE_ADDR') || DEFAULT_CORE_ADDR
}

function parseAddress(address) {
  if (address.includes('://') || /\s/.test(address)) return null
  try {
    const parsed = new URL(`http://${address}`)
    if (!parsed.hostname || !parsed.port) return null
    return { host: parsed.hostname.replace(/^\[|\]$/g, ''), port: Number(parsed.port) }
  } catch {
    return null
  }
}

export function resolveCoreBaseUrl(env) {
  return valueOf(env, 'NINEPROFS_CORE_URL') || `http://${coreAddress(env)}`
}

export function validateCoreConfiguration(env) {
  const address = coreAddress(env)
  const parsedAddress = parseAddress(address)
  if (!parsedAddress) return { ok: false, reason: 'Set NINEPROFS_CORE_ADDR as host:port.' }
  const baseUrl = resolveCoreBaseUrl(env)
  try {
    const parsedUrl = new URL(baseUrl)
    if (!['http:', 'https:'].includes(parsedUrl.protocol) || !parsedUrl.hostname) throw new Error()
  } catch {
    return { ok: false, reason: 'Set NINEPROFS_CORE_URL to an http(s) URL or leave it empty.' }
  }
  const dataDir = valueOf(env, 'NINEPROFS_CORE_DATA_DIR') || 'data/9profs-core'
  if (!dataDir) return { ok: false, reason: 'Set NINEPROFS_CORE_DATA_DIR to a writable directory.' }
  return { ok: true, address, baseUrl, dataDir, parsedAddress }
}

function endpoint(baseUrl, path) {
  return `${baseUrl.replace(/\/+$/, '')}${path}`
}

async function fetchJson(url, fetchImpl, timeoutMs) {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeoutMs)
  try {
    const response = await fetchImpl(url, { signal: controller.signal })
    let body = null
    try {
      body = await response.json()
    } catch {
      body = null
    }
    return { response, body }
  } finally {
    clearTimeout(timer)
  }
}

export async function probeCore(baseUrl, { fetchImpl = globalThis.fetch, timeoutMs = 2_000 } = {}) {
  try {
    const health = await fetchJson(endpoint(baseUrl, '/api/health'), fetchImpl, timeoutMs)
    if (!health.response.ok) {
      return {
        reachable: true,
        compatible: false,
        reason: `health returned HTTP ${health.response.status}`,
      }
    }
    if (
      health.body?.success !== true ||
      health.body?.data?.status !== 'ok' ||
      health.body?.data?.service !== '9profs-core'
    ) {
      return {
        reachable: true,
        compatible: false,
        reason: 'health response is not a 9Profs Core response',
      }
    }

    const runtime = await fetchJson(endpoint(baseUrl, '/api/runtime'), fetchImpl, timeoutMs)
    if (!runtime.response.ok) {
      return {
        reachable: true,
        compatible: false,
        reason: `runtime returned HTTP ${runtime.response.status}`,
      }
    }
    if (
      runtime.body?.success !== true ||
      runtime.body?.data?.service !== '9profs-core' ||
      runtime.body?.data?.protocol_version !== '1' ||
      !runtime.body?.data?.capabilities?.includes('research')
    ) {
      return {
        reachable: true,
        compatible: false,
        reason: 'runtime response is not a compatible Research Core',
      }
    }
    return { reachable: true, compatible: true, reason: '' }
  } catch {
    return { reachable: false, compatible: false, reason: 'Core did not answer' }
  }
}

export function isTcpPortOpen(host, port, timeoutMs = 500) {
  return new Promise((resolveResult) => {
    const socket = createConnection({ host, port })
    let settled = false
    const finish = (value) => {
      if (settled) return
      settled = true
      socket.destroy()
      resolveResult(value)
    }
    socket.once('connect', () => finish(true))
    socket.once('error', () => finish(false))
    socket.setTimeout(timeoutMs, () => finish(false))
  })
}

export async function inspectCore(
  baseUrl,
  { probe = probeCore, portOpen = isTcpPortOpen, timeoutMs = 500 } = {},
) {
  const result = await probe(baseUrl)
  if (result.compatible || result.reachable) return { ...result, occupied: true }
  let occupied = false
  try {
    const parsed = new URL(baseUrl)
    if (parsed.port) occupied = await portOpen(parsed.hostname, Number(parsed.port), timeoutMs)
  } catch {
    occupied = false
  }
  return { ...result, occupied }
}

const sleep = (milliseconds) =>
  new Promise((resolveResult) => setTimeout(resolveResult, milliseconds))

export async function waitForCore(
  baseUrl,
  {
    probe = probeCore,
    child,
    timeoutMs = DEFAULT_CORE_TIMEOUT_MS,
    pollIntervalMs = DEFAULT_POLL_INTERVAL_MS,
    sleepImpl = sleep,
  } = {},
) {
  const deadline = Date.now() + timeoutMs
  let childError = null
  const onChildError = (error) => {
    childError = error
  }
  child?.once('error', onChildError)
  try {
    while (Date.now() <= deadline) {
      if (childError) throw new Error(`9Profs Core could not start: ${childError.message}`)
      const result = await probe(baseUrl)
      if (result.compatible) return result
      if (result.reachable) throw new Error(`Core at ${baseUrl} is not a compatible 9Profs Core.`)
      if (child?.exitCode !== null && child?.exitCode !== undefined) {
        throw new Error(`9Profs Core exited before becoming ready (exit code ${child.exitCode}).`)
      }
      await sleepImpl(Math.min(pollIntervalMs, Math.max(1, deadline - Date.now())))
    }
  } finally {
    child?.removeListener('error', onChildError)
  }
  throw new Error(`Timed out waiting for 9Profs Core at ${baseUrl}.`)
}

export function buildNpmCommand(
  script,
  {
    platform = process.platform,
    execPath = process.execPath,
    npmExecPath = process.env.npm_execpath,
    comSpec = process.env.ComSpec || 'cmd.exe',
    extraArgs = [],
  } = {},
) {
  const npmArgs = ['run', script, ...extraArgs]
  if (npmExecPath) return { command: execPath, args: [npmExecPath, ...npmArgs] }
  if (platform === 'win32') {
    return { command: comSpec, args: ['/d', '/s', '/c', 'npm.cmd', ...npmArgs] }
  }
  return { command: 'npm', args: npmArgs }
}

export function launchNpm(script, rootDir, env, spawnImpl = defaultSpawn, commandOptions = {}) {
  const command = buildNpmCommand(script, commandOptions)
  return spawnImpl(command.command, command.args, {
    cwd: rootDir,
    env,
    stdio: 'inherit',
    shell: false,
    detached: process.platform !== 'win32',
    windowsHide: false,
  })
}

export function terminateChild(
  child,
  { platform = process.platform, execFileSyncImpl = execFileSync } = {},
) {
  if (!child || child.exitCode !== null) return
  if (platform === 'win32' && child.pid) {
    try {
      execFileSyncImpl('taskkill', ['/pid', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
      return
    } catch {
      // Fall through when taskkill cannot find an already-exited process.
    }
  } else if (child.pid) {
    try {
      process.kill(-child.pid, 'SIGTERM')
      return
    } catch {
      // Fall through when the process group has already exited.
    }
  }
  child.kill?.('SIGTERM')
}

function localEndpointMatches(env) {
  const configured = new URL(resolveCoreBaseUrl(env))
  const launched = new URL(`http://${coreAddress(env)}`)
  const localNames = new Set(['localhost', '127.0.0.1', '::1', '[::1]'])
  const configuredHost = configured.hostname.replace(/^\[|\]$/g, '')
  const launchedHost = launched.hostname.replace(/^\[|\]$/g, '')
  return (
    (configured.protocol === launched.protocol &&
      configured.port === launched.port &&
      configuredHost === launchedHost) ||
    (configured.protocol === launched.protocol &&
      configured.port === launched.port &&
      localNames.has(configuredHost) &&
      localNames.has(launchedHost))
  )
}

export async function ensureCore({
  rootDir = ROOT_DIR,
  env,
  probe = probeCore,
  portOpen = isTcpPortOpen,
  spawnImpl = defaultSpawn,
  sleepImpl = sleep,
  timeoutMs = DEFAULT_CORE_TIMEOUT_MS,
  pollIntervalMs = DEFAULT_POLL_INTERVAL_MS,
  log = console.log,
} = {}) {
  const config = validateCoreConfiguration(env)
  if (!config.ok) throw new Error(config.reason)
  const initial = await inspectCore(config.baseUrl, { probe, portOpen })
  if (initial.compatible) {
    log(`Reusing compatible 9Profs Core at ${config.baseUrl}.`)
    return { baseUrl: config.baseUrl, child: null, started: false }
  }
  if (initial.occupied) {
    throw new Error(
      `Core address ${config.baseUrl} is occupied, but it is not a compatible 9Profs Core. ` +
        'Stop that process or change NINEPROFS_CORE_ADDR.',
    )
  }
  if (valueOf(env, 'NINEPROFS_CORE_URL') && !localEndpointMatches(env)) {
    throw new Error(
      'NINEPROFS_CORE_URL is unavailable and does not match NINEPROFS_CORE_ADDR; ' +
        'start the external Core or configure one local address.',
    )
  }

  const child = launchNpm('core:run', rootDir, env, spawnImpl)
  try {
    await waitForCore(config.baseUrl, { probe, child, sleepImpl, timeoutMs, pollIntervalMs })
  } catch (error) {
    terminateChild(child)
    throw error
  }
  log(`9Profs Core ready at ${config.baseUrl}.`)
  return { baseUrl: config.baseUrl, child, started: true }
}

export function superviseChildren(
  appChild,
  coreChild,
  { signalSource = process, terminate = terminateChild } = {},
) {
  return new Promise((resolveResult) => {
    let settled = false
    const signals = ['SIGINT', 'SIGTERM']
    const removeListeners = () => {
      appChild?.removeListener?.('exit', onAppExit)
      appChild?.removeListener?.('error', onAppError)
      coreChild?.removeListener?.('exit', onCoreExit)
      for (const signal of signals) signalSource.removeListener?.(signal, onSignal)
    }
    const finish = (code) => {
      if (settled) return
      settled = true
      removeListeners()
      if (coreChild) terminate(coreChild)
      resolveResult(code)
    }
    const onAppExit = (code) => finish(code ?? 1)
    const onAppError = () => finish(1)
    const onCoreExit = () => {
      if (settled) return
      terminate(appChild)
      finish(1)
    }
    const onSignal = () => {
      terminate(appChild)
      finish(130)
    }
    appChild.once('exit', onAppExit)
    appChild.once('error', onAppError)
    coreChild?.once('exit', onCoreExit)
    for (const signal of signals) signalSource.once?.(signal, onSignal)
  })
}

export async function runDev({
  rootDir = ROOT_DIR,
  baseEnv = process.env,
  spawnImpl = defaultSpawn,
  probe = probeCore,
  portOpen = isTcpPortOpen,
  sleepImpl = sleep,
  log = console.log,
} = {}) {
  const { env, fileExists } = loadDogfoodingEnv(rootDir, baseEnv)
  if (!fileExists) log(`Missing ${LOCAL_ENV_FILENAME}; run npm run setup:9profs first.`)
  const core = await ensureCore({ rootDir, env, probe, portOpen, spawnImpl, sleepImpl, log })
  let appChild
  try {
    log('Starting existing 9Profs renderer and Electron development stack.')
    appChild = launchNpm('dev', rootDir, env, spawnImpl)
    return await superviseChildren(appChild, core.child)
  } catch (error) {
    terminateChild(core.child)
    throw error
  }
}

export async function runCore({
  rootDir = ROOT_DIR,
  baseEnv = process.env,
  spawnImpl = defaultSpawn,
  log = console.log,
} = {}) {
  const { env, fileExists } = loadDogfoodingEnv(rootDir, baseEnv)
  if (!fileExists) log(`Missing ${LOCAL_ENV_FILENAME}; run npm run setup:9profs first.`)
  const command = process.platform === 'win32' ? 'cargo.exe' : 'cargo'
  const child = spawnImpl(
    command,
    [
      'run',
      '--manifest-path',
      join(rootDir, '9profs-core-rs', 'Cargo.toml'),
      '--bin',
      'nineprofs-core',
    ],
    {
      cwd: rootDir,
      env,
      stdio: 'inherit',
      shell: false,
      windowsHide: false,
    },
  )
  return await new Promise((resolve) => {
    let settled = false
    const finish = (code) => {
      if (settled) return
      settled = true
      resolve(code)
    }
    child.once('exit', (code, signal) => finish(signal ? 1 : code ?? 1))
    child.once('error', () => finish(1))
  })
}

async function checkDify(env, coreBaseUrl, { fetchImpl = globalThis.fetch } = {}) {
  const baseUrl = valueOf(env, 'NINEPROFS_DIFY_BASE_URL')
  const apiKey = valueOf(env, 'NINEPROFS_DIFY_API_KEY')
  if (!baseUrl || !apiKey) {
    return {
      status: 'NOT CONFIGURED',
      details: [
        'Set NINEPROFS_DIFY_BASE_URL and NINEPROFS_DIFY_API_KEY for citation verification.',
        'Citation verification unavailable; other Research analysis may still work.',
      ],
    }
  }
  try {
    const parsed = new URL(baseUrl)
    if (!['http:', 'https:'].includes(parsed.protocol) || !parsed.hostname) throw new Error()
  } catch {
    return { status: 'NOT READY', details: ['Set NINEPROFS_DIFY_BASE_URL to an http(s) URL.'] }
  }
  try {
    const result = await fetchJson(
      endpoint(coreBaseUrl, `/api/research/cases/${DOCTOR_RETRIEVAL_CASE_ID}/retrieval-index`),
      fetchImpl,
      2_000,
    )
    if (!result.response.ok || result.body?.success !== true) {
      return {
        status: 'NOT READY',
        details: ['Core retrieval readiness probe failed; check Core logs.'],
      }
    }
    const readiness = result.body?.data?.readiness
    if (readiness?.ready === true && readiness.status === 'ready')
      return { status: 'READY', details: [] }
    if (readiness?.status === 'unauthorized') {
      return { status: 'NOT READY', details: ['Dify rejected the configured API key.'] }
    }
    if (readiness?.status === 'unreachable') {
      return {
        status: 'NOT READY',
        details: ['Dify is unreachable; check NINEPROFS_DIFY_BASE_URL.'],
      }
    }
    return {
      status: 'NOT READY',
      details: ['Dify is configured but not ready; check Dify and Core logs.'],
    }
  } catch {
    return {
      status: 'NOT READY',
      details: ['Dify readiness probe failed; check Core and Dify connectivity.'],
    }
  }
}

export async function createDoctorReport({
  rootDir = ROOT_DIR,
  baseEnv = process.env,
  probe = probeCore,
  portOpen = isTcpPortOpen,
  fetchImpl = globalThis.fetch,
} = {}) {
  const { env, fileExists } = loadDogfoodingEnv(rootDir, baseEnv)
  const coreConfig = validateCoreConfiguration(env)
  const report = {
    fileExists,
    coreConfig,
    core: null,
    models: MODEL_TASKS.map((task) => ({ task, ...evaluateModelReadiness(env, task) })),
    dify: null,
    exitCode: 0,
  }
  if (!coreConfig.ok) {
    report.exitCode = 1
    return report
  }
  report.core = await inspectCore(coreConfig.baseUrl, { probe, portOpen })
  if (!report.core.compatible) report.exitCode = 1
  report.dify = report.core.compatible
    ? await checkDify(env, coreConfig.baseUrl, { fetchImpl })
    : valueOf(env, 'NINEPROFS_DIFY_BASE_URL') && valueOf(env, 'NINEPROFS_DIFY_API_KEY')
      ? { status: 'NOT CHECKED', details: ['Core is unavailable; rerun doctor after Core starts.'] }
      : {
          status: 'NOT CONFIGURED',
          details: [
            'Set NINEPROFS_DIFY_BASE_URL and NINEPROFS_DIFY_API_KEY for citation verification.',
            'Citation verification unavailable; other Research analysis may still work.',
          ],
        }
  return report
}

function reportStatus(lines, label, status, details = []) {
  lines.push(`${label.padEnd(28)}${status}`)
  for (const detail of details) lines.push(`${''.padEnd(28)}${detail}`)
}

export function formatDoctorReport(report) {
  const lines = ['9Profs Dogfooding Readiness', '']
  if (!report.fileExists) lines.push(`Configuration file missing; run npm run setup:9profs.`)
  if (report.coreConfig.ok) {
    reportStatus(lines, 'Core configuration', 'OK')
    reportStatus(
      lines,
      'Core reachable',
      report.core?.compatible ? 'OK' : report.core?.reachable ? 'NOT COMPATIBLE' : 'NOT REACHABLE',
      report.core?.compatible
        ? []
        : [
            report.core?.reachable
              ? 'Stop incompatible process or change NINEPROFS_CORE_ADDR.'
              : 'Start with npm run dev:9profs.',
          ],
    )
  } else {
    reportStatus(lines, 'Core configuration', 'NOT READY', [report.coreConfig.reason])
    reportStatus(lines, 'Core reachable', 'NOT CHECKED')
  }
  lines.push('')
  for (const model of report.models)
    reportStatus(lines, model.task.label, model.status, model.reason ? [model.reason] : [])
  lines.push('')
  reportStatus(lines, 'Dify', report.dify?.status || 'NOT CHECKED', report.dify?.details || [])
  lines.push('')
  lines.push(
    report.exitCode === 0
      ? 'Core is ready. Model and Dify rows show which Research levels are available.'
      : 'Fix Core configuration/reachability, then run npm run doctor:9profs again.',
  )
  return lines.join('\n')
}

export async function main(argv = process.argv) {
  const command = argv[2] || 'help'
  if (command === 'setup') {
    const result = setupLocalEnv()
    console.log(
      result.created
        ? `Created ${LOCAL_ENV_FILENAME}.`
        : `Preserved existing ${LOCAL_ENV_FILENAME}.`,
    )
    console.log(`Edit ${LOCAL_ENV_FILENAME}, then run:`)
    console.log('  npm run doctor:9profs')
    console.log('  npm run dev:9profs')
    return 0
  }
  if (command === 'doctor') {
    const report = await createDoctorReport()
    console.log(formatDoctorReport(report))
    return report.exitCode
  }
  if (command === 'dev') return runDev()
  if (command === 'core') return runCore()
  console.log('Usage: node scripts/9profs.mjs <setup|doctor|dev|core>')
  return 0
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
  try {
    const result = await main()
    if (typeof result === 'number') process.exitCode = result
  } catch (error) {
    console.error(
      `9Profs dogfooding command failed: ${error instanceof Error ? error.message : String(error)}`,
    )
    process.exitCode = 1
  }
}
