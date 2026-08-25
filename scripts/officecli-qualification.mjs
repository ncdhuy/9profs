import { existsSync, mkdtempSync } from 'node:fs'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { tmpdir } from 'node:os'

const pinnedVersion = '1.0.144'
const binary = process.env.NINEPROFS_OFFICECLI_PATH

if (!binary || !existsSync(binary)) {
  console.error(`qualification requires NINEPROFS_OFFICECLI_PATH pointing to OfficeCLI v${pinnedVersion}`)
  process.exit(1)
}

const root = mkdtempSync(join(tmpdir(), '9profs-officecli-qualification-'))
const env = {
  ...process.env,
  NINEPROFS_OFFICECLI_PATH: binary,
  NINEPROFS_OFFICECLI_PROFILE: join(root, 'profile'),
  NINEPROFS_OFFICECLI_ARTIFACT_ROOT: join(root, 'artifacts'),
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
