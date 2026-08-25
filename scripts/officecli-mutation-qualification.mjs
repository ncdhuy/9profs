import { existsSync, mkdtempSync, rmSync } from 'node:fs'
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
    `mutation qualification requires NINEPROFS_OFFICECLI_PATH pointing to OfficeCLI v${pinnedVersion}`,
  )
  process.exit(1)
}
if (!existsSync(electron) || !existsSync(rasterizerScript)) {
  console.error(
    'mutation qualification requires the existing Electron runtime and 9Profs HTML rasterizer script',
  )
  process.exit(1)
}

const root = mkdtempSync(join(tmpdir(), '9profs-officecli-mutation-qualification-'))
const env = {
  ...process.env,
  NINEPROFS_OFFICECLI_PATH: binary,
  NINEPROFS_OFFICECLI_PROFILE: join(root, 'profile'),
  NINEPROFS_OFFICECLI_ARTIFACT_ROOT: join(root, 'artifacts'),
  NINEPROFS_ELECTRON_PATH: electron,
  NINEPROFS_HTML_RASTERIZER_SCRIPT: rasterizerScript,
  NINEPROFS_OFFICECLI_MUTATION_QUALIFICATION: '1',
  OFFICECLI_NO_AUTO_RESIDENT: '1',
  OFFICECLI_NO_AUTO_INSTALL: '1',
  OFFICECLI_SKIP_UPDATE: '1',
}

const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo'
let status = 1
try {
  const result = spawnSync(
    cargo,
    [
      'test',
      '--manifest-path',
      '9profs-core-rs/Cargo.toml',
      '-p',
      'nineprofs-officecli',
      '--test',
      'mutation_qualification',
      '--',
      '--nocapture',
    ],
    { cwd: process.cwd(), env, stdio: 'inherit' },
  )

  if (result.error) {
    console.error(`mutation qualification could not start cargo: ${result.error.message}`)
  } else {
    status = result.status ?? 1
  }
} finally {
  rmSync(root, { recursive: true, force: true })
}
process.exit(status)
