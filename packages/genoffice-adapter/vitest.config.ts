import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitest/config'

const local = (rel: string) => fileURLToPath(new URL(rel, import.meta.url))

export default defineConfig({
  resolve: {
    alias: {
      '@genoffice/document-gateway': local('../document-gateway/src/index.ts'),
    },
  },
  test: {
    include: ['tests/**/*.test.ts'],
    environment: 'node',
  },
})
