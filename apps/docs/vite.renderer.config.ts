import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'
import { resolve } from 'node:path'

// renderer-only dev server (embedded by the shell via DOCS_RENDERER_URL for HMR; no standalone Electron)
export default defineConfig({
  root: 'src/renderer',
  plugins: [react()],
  resolve: {
    alias: {
      '@genoffice/document-gateway': resolve(
        __dirname,
        '../../packages/document-gateway/src/index.ts',
      ),
      '@genoffice/genoffice-adapter': resolve(
        __dirname,
        '../../packages/genoffice-adapter/src/index.ts',
      ),
    },
  },
  server: {
    port: Number(process.env.DOCS_DEV_PORT) || 5173,
    strictPort: true,
  },
})
