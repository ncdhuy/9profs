const { statSync, writeFileSync } = require('node:fs')
const { basename, join, resolve } = require('node:path')
const { app, BrowserWindow, session } = require('electron')

const args = new Map()
for (let index = 2; index < process.argv.length; index += 2) {
  const key = process.argv[index]
  const value = process.argv[index + 1]
  if (!key?.startsWith('--') || value === undefined) {
    throw new Error('invalid rasterizer arguments')
  }
  args.set(key.slice(2), value)
}

const htmlPath = resolve(normalizeWindowsPath(required('html')))
const outputRoot = resolve(normalizeWindowsPath(required('output-root')))
const prefix = required('prefix')
const manifestName = required('manifest')
const maxDimension = boundedInteger('max-dimension', 1)
const maxPages = boundedInteger('max-pages', 1)
const maxTotalBytes = boundedInteger('max-total-bytes', 1)
const selectedPage = args.has('page') ? boundedInteger('page', 1) : undefined
const viewportWidth = args.has('viewport-width')
  ? boundedInteger('viewport-width', 1)
  : maxDimension
const viewportHeight = args.has('viewport-height')
  ? boundedInteger('viewport-height', 1)
  : maxDimension

if (!statSync(htmlPath).isFile()) throw new Error('HTML artifact is not a file')
if (basename(htmlPath).includes('..')) throw new Error('invalid HTML artifact name')

const network = { blocked: 0 }
const blockedProtocols = new Set(['http:', 'https:', 'ws:', 'wss:'])
const timeoutMs = 5000
const settleMs = 100
let window

async function main() {
  try {
    app.setPath('userData', join(outputRoot, '.electron-user-data'))
    app.setPath('sessionData', join(outputRoot, '.electron-session-data'))
    app.setPath('cache', join(outputRoot, '.electron-cache'))
    app.commandLine.appendSwitch('disable-gpu')
    await app.whenReady()
    session.defaultSession.webRequest.onBeforeRequest(
      { urls: ['http://*/*', 'https://*/*', 'ws://*/*', 'wss://*/*'] },
      (details, callback) => {
        if (blockedProtocols.has(new URL(details.url).protocol)) network.blocked += 1
        callback({ cancel: true })
      },
    )
    window = new BrowserWindow({
      show: false,
      width: Math.min(viewportWidth, maxDimension),
      height: Math.min(viewportHeight, maxDimension),
      webPreferences: { contextIsolation: true, sandbox: true, offscreen: true },
    })
    await withTimeout(window.loadFile(htmlPath), timeoutMs, 'HTML load timed out')
    await withTimeout(
      window.webContents.executeJavaScript(`
    (() => {
      if (!document.documentElement || document.documentElement.tagName !== 'HTML' || !document.body) throw new Error('invalid HTML document')
      return true
    })()
  `),
      timeoutMs,
      'HTML validation timed out',
    )
    await new Promise((resolvePromise) => setTimeout(resolvePromise, settleMs))

    const nodes = await withTimeout(
      window.webContents.executeJavaScript(`
    (() => {
      const groups = [
        ['page', '.page[data-page]'],
        ['sheet', '.sheet-content[data-sheet]'],
        ['slide', '.slide-container[data-slide]'],
      ]
      for (const [kind, selector] of groups) {
        const found = [...document.querySelectorAll(selector)]
        if (found.length) {
          found.forEach((element) => {
            element.style.setProperty('display', 'block', 'important')
            element.style.setProperty('visibility', 'visible', 'important')
          })
          return found.map((element, index) => ({ kind, index, selector }))
        }
      }
      return [{ kind: 'document', index: 0, selector: 'body' }]
    })()
  `),
      timeoutMs,
      'HTML inspection timed out',
    )
    const chosen =
      selectedPage === undefined ? nodes : nodes.filter((node) => node.index + 1 === selectedPage)
    if (!chosen.length) throw new Error('requested page or slide is unavailable')
    if (chosen.length > maxPages) throw new Error('rendered page limit exceeded')

    const artifacts = []
    let totalBytes = 0
    for (const node of chosen) {
      const geometry = await withTimeout(
        window.webContents.executeJavaScript(`
      (() => {
        const element = document.querySelectorAll(${JSON.stringify(node.selector)})[${node.index}]
        if (!element) throw new Error('render target is unavailable')
        element.scrollIntoView({ block: 'start', inline: 'start' })
        const rect = element.getBoundingClientRect()
        return { x: rect.x + window.scrollX, y: rect.y + window.scrollY, width: rect.width, height: rect.height }
      })()
    `),
        timeoutMs,
        'render geometry timed out',
      )
      const width = Math.ceil(geometry.width)
      const height = Math.ceil(geometry.height)
      if (!width || !height || width > maxDimension || height > maxDimension)
        throw new Error('render dimensions exceed configured limit')
      const image = await withTimeout(
        window.webContents.capturePage({ x: geometry.x, y: geometry.y, width, height }),
        timeoutMs,
        'PNG capture timed out',
      )
      const png = image.toPNG()
      const imageWidth = png.length >= 24 ? png.readUInt32BE(16) : 0
      const imageHeight = png.length >= 24 ? png.readUInt32BE(20) : 0
      if (
        png.length < 100 ||
        png.readUInt32BE(0) !== 0x89504e47 ||
        !imageWidth ||
        !imageHeight ||
        imageWidth > maxDimension ||
        imageHeight > maxDimension
      )
        throw new Error('PNG capture is empty or exceeds configured dimensions')
      totalBytes += png.length
      if (totalBytes > maxTotalBytes) throw new Error('rendered artifact byte limit exceeded')
      const id = `${prefix}-${node.kind}-${node.index + 1}`
      const name = `${id}.png`
      const destination = resolve(outputRoot, name)
      if (!destination.startsWith(outputRoot + '\\') && !destination.startsWith(outputRoot + '/'))
        throw new Error('output escaped artifact root')
      writeFileSync(destination, png, { flag: 'wx' })
      artifacts.push({
        id,
        name,
        kind: node.kind,
        index: node.index + 1,
        width: imageWidth,
        height: imageHeight,
        bytes: png.length,
      })
    }
    const manifestPath = resolve(outputRoot, manifestName)
    if (!manifestPath.startsWith(outputRoot + '\\') && !manifestPath.startsWith(outputRoot + '/'))
      throw new Error('manifest escaped artifact root')
    writeFileSync(
      manifestPath,
      JSON.stringify({ artifacts, blocked_network_requests: network.blocked }),
      { flag: 'wx' },
    )
    process.stdout.write('\r\n')
  } catch (error) {
    process.stderr.write(`${error?.message ?? error}\n`)
    process.exitCode = 1
  } finally {
    if (window && !window.isDestroyed()) window.destroy()
    if (app.isReady()) app.quit()
  }
}

main().catch((error) => {
  process.stderr.write(`${error?.message ?? error}\n`)
  app.quit()
  process.exitCode = 1
})

function required(name) {
  const value = args.get(name)
  if (!value) throw new Error(`missing --${name}`)
  return value
}

function normalizeWindowsPath(value) {
  return value.startsWith('\\\\?\\') ? value.slice(4) : value
}

function boundedInteger(name, minimum) {
  const value = Number.parseInt(required(name), 10)
  if (!Number.isSafeInteger(value) || value < minimum) throw new Error(`invalid --${name}`)
  return value
}

function withTimeout(promise, milliseconds, message) {
  return Promise.race([
    promise,
    new Promise((_, reject) => setTimeout(() => reject(new Error(message)), milliseconds)),
  ])
}
