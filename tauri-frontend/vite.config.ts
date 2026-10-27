import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Tauri 前端独立构建（与主工程 src/ 并存；产物进 dist/）
// 因此放开对上级目录的 fs 访问。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
    fs: { allow: ['..'] },
    watch: { ignored: ['**/src-tauri/**'] }
  },
  build: {
    outDir: 'dist',
    target: 'chrome105',
    minify: 'esbuild'
  }
})
