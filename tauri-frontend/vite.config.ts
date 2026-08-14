import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Tauri 前端独立构建（与主工程 src/ 并存；产物进 dist/）
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] }
  },
  build: {
    outDir: 'dist',
    target: 'chrome105',
    minify: 'esbuild'
  }
})
