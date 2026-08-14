import { resolve } from 'path'
import { defineConfig } from 'electron-vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  main: {
    build: {
      outDir: 'out/main',
      lib: {
        entry: {
          index: resolve(__dirname, 'electron/main.ts'),
          'plugin-process': resolve(__dirname, 'plugin-system/PluginProcessEntry.ts')
        },
        formats: ['cjs']
      }
    },
    resolve: {
      alias: {
        '@shared': resolve(__dirname, 'shared'),
        '@database': resolve(__dirname, 'database')
      }
    }
  },
  preload: {
    build: {
      outDir: 'out/preload',
      externalizeDeps: false,
      lib: {
        entry: resolve(__dirname, 'electron/preload.ts'),
        formats: ['cjs']
      }
    },
    resolve: {
      alias: {
        '@shared': resolve(__dirname, 'shared')
      }
    }
  },
  renderer: {
    root: __dirname,
    build: {
      outDir: 'out/renderer',
      manifest: true,
      rollupOptions: {
        input: resolve(__dirname, 'index.html')
      }
    },
    resolve: {
      alias: {
        '@shared': resolve(__dirname, 'shared'),
        '@': resolve(__dirname, 'src')
      }
    },
    plugins: [react()]
  }
})
