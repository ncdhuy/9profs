import { resolve } from 'node:path'
import react from '@vitejs/plugin-react'
import { defineConfig, externalizeDepsPlugin } from 'electron-vite'

// Resolve workspace packages from this checkout's sources: in a git worktree
// node_modules is a symlink into the main checkout, so bare specifiers would
// silently bundle the other checkout's (possibly stale) code.
const localAlias = {
  '@genoffice/9profs-core': resolve(__dirname, '../../packages/9profs-core/src/index.ts'),
  '@genoffice/docx-engine': resolve(__dirname, '../../packages/docx-engine/src/index.ts'),
  '@genoffice/document-gateway': resolve(__dirname, '../../packages/document-gateway/src/index.ts'),
  '@genoffice/genoffice-adapter': resolve(
    __dirname,
    '../../packages/genoffice-adapter/src/index.ts',
  ),
}

export default defineConfig({
  // Main and preload use only electron + node builtins; bundle everything so
  // the packaged app doesn't rely on node_modules at runtime.
  // @genoffice/* deps ship as raw TS source with extensionless imports, so they
  // must be bundled — externalizing them yields ERR_MODULE_NOT_FOUND under Node
  // (same setup as apps/slides).
  main: {
    plugins: [
      externalizeDepsPlugin({ exclude: ['@genoffice/electron-utils', '@genoffice/font-metrics'] }),
    ],
    resolve: { alias: localAlias },
  },
  preload: {},
  renderer: {
    plugins: [react()],
    resolve: { alias: localAlias },
    server: {
      // Overridable so multiple genoffice dev instances can coexist (default 5173).
      port: Number(process.env.DOCS_DEV_PORT) || 5173,
      strictPort: Boolean(process.env.DOCS_DEV_PORT),
    },
  },
})
