import { spawn } from 'node:child_process'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { buildNpmCommand } from './9profs.mjs'

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const env = {
  ...process.env,
  DOCS_RENDERER_URL: 'http://localhost:5173',
  SHEETS_RENDERER_URL: 'http://localhost:5174',
  SLIDES_RENDERER_URL: 'http://localhost:5175',
  PDF_RENDERER_URL: 'http://localhost:5176',
  MARKDOWN_RENDERER_URL: 'http://localhost:5177',
}
const command = buildNpmCommand('dev', { extraArgs: ['-w', '@genoffice/shell'] })
const child = spawn(command.command, command.args, {
  cwd: rootDir,
  env,
  stdio: 'inherit',
  shell: false,
  windowsHide: false,
})

const stop = () => child.kill()
process.once('SIGINT', stop)
process.once('SIGTERM', stop)
child.once('error', (error) => {
  console.error(`Shell development command failed: ${error.message}`)
  process.exitCode = 1
})
child.once('exit', (code) => {
  process.removeListener('SIGINT', stop)
  process.removeListener('SIGTERM', stop)
  process.exitCode = code ?? 1
})
