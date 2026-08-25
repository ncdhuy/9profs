import { existsSync, mkdtempSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'

const pinnedVersion = '1.0.144'
const binary = process.env.NINEPROFS_OFFICECLI_PATH
const require = createRequire(import.meta.url)
const electronPackage = require.resolve('electron/package.json')
const electron =
  process.env.NINEPROFS_ELECTRON_PATH ??
  join(dirname(electronPackage), 'dist', process.platform === 'win32' ? 'electron.exe' : 'electron')
const rasterizerScript = join(process.cwd(), 'scripts', 'html-rasterizer.cjs')

if (!binary || !existsSync(binary)) {
  console.error(
    `qualification requires NINEPROFS_OFFICECLI_PATH pointing to OfficeCLI v${pinnedVersion}`,
  )
  process.exit(1)
}
if (!existsSync(electron) || !existsSync(rasterizerScript)) {
  console.error(
    'qualification requires the existing Electron runtime and 9Profs HTML rasterizer script',
  )
  process.exit(1)
}

const root = mkdtempSync(join(tmpdir(), '9profs-officecli-qualification-'))
const env = {
  ...process.env,
  NINEPROFS_OFFICECLI_PATH: binary,
  NINEPROFS_OFFICECLI_PROFILE: join(root, 'profile'),
  NINEPROFS_OFFICECLI_ARTIFACT_ROOT: join(root, 'artifacts'),
  NINEPROFS_ELECTRON_PATH: electron,
  NINEPROFS_HTML_RASTERIZER_SCRIPT: rasterizerScript,
  NINEPROFS_RASTERIZER_QUALIFICATION: '1',
  NINEPROFS_OFFICECLI_QUALIFICATION: '1',
}

const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo'
const result = spawnSync(
  cargo,
  [
    'test',
    '--manifest-path',
    '9profs-core-rs/Cargo.toml',
    '-p',
    'nineprofs-officecli',
    '--tests',
    '--',
    '--nocapture',
  ],
  { cwd: process.cwd(), env, stdio: 'inherit' },
)

if (result.error) {
  console.error(`qualification could not start cargo: ${result.error.message}`)
  process.exit(1)
}
process.exit(result.status ?? 1)
