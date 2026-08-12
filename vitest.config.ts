import { defineConfig } from 'vitest/config'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const repositoryRoot = dirname(fileURLToPath(import.meta.url))

export default defineConfig({
  resolve: {
    alias: {
      '@shared': resolve(repositoryRoot, 'shared'),
      '@database': resolve(repositoryRoot, 'database')
    }
  },
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    globals: true
  }
})
