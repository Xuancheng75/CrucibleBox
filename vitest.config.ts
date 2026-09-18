import { defineConfig } from 'vitest/config'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const repositoryRoot = dirname(fileURLToPath(import.meta.url))

export default defineConfig({
  resolve: {
    alias: {
      '@shared': resolve(repositoryRoot, 'shared')
    }
  },
  test: {
    environment: 'node',
    include: [
      'tests/antdMigration.test.ts',
      'tests/appPages.test.ts',
      'tests/drop-target.test.ts',
      'tests/homeErrorHandling.test.ts',
      'tests/openboxUi.test.ts',
      'tests/pluginFrameBridge.test.ts',
      'tests/pluginGroupReorder.test.ts',
      'tests/pluginRendererRpc.test.ts',
      'tests/pluginSortOrder.test.ts',
      'tests/pluginSources.test.ts',
      'tests/pluginStorageConsumers.test.ts',
      'tests/themes.test.ts',
      'tests/updateCheck.test.ts'
    ],
    globals: true
  }
})
