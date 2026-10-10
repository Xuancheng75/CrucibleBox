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
      'tests/backendConfigRpc.test.ts',
      'tests/dataConversion.test.ts',
      'tests/appPages.test.ts',
      'tests/drop-target.test.ts',
      'tests/homeErrorHandling.test.ts',
      'tests/hostTaskStore.test.ts',
      'tests/marketplaceVersionCompare.test.ts',
      'tests/openboxUi.test.ts',
      'tests/pluginFrameBridge.test.ts',
      'tests/nextFrameBridge.test.ts',
      'tests/nextThemeController.test.ts',
      'tests/nextUiController.test.ts',
      'tests/nextFrameAppearance.test.ts',
      'tests/nextSessionRequest.test.ts',
      'tests/themeCacheRetention.test.ts',
      'tests/nextOfficialCatalog.test.ts',
      'tests/tauriRuntimeAssets.test.ts',
      'tests/pluginGroupReorder.test.ts',
      'tests/pluginRendererRpc.test.ts',
      'tests/pluginSearch.test.ts',
      'tests/pluginSortOrder.test.ts',
      'tests/pluginSources.test.ts',
      'tests/pluginStorageConsumers.test.ts',
      'tests/themes.test.ts',
      'tests/updateCheck.test.ts'
    ],
    globals: true
  }
})
