import './styles.css'
import { createClient } from '@cruciblebox/next-api'
export async function mount({ root, session, exchange }) {
  const client = createClient({ session, exchange })
  const button = document.createElement('button')
  button.textContent = 'Ping host'
  const output = document.createElement('pre')
  const workerButton = document.createElement('button')
  workerButton.textContent = 'Load resource worker'
  let exportUrl
  const exportButton = document.createElement('button')
  exportButton.textContent = 'Export example'
  exportButton.onclick = () => {
    if (exportUrl) URL.revokeObjectURL(exportUrl)
    exportUrl = URL.createObjectURL(
      new Blob(['Next browser download proof\n'], { type: 'text/plain' })
    )
    const a = document.createElement('a')
    a.href = exportUrl
    a.download = 'next-export-proof.txt'
    a.click()
  }
  let workerUrl
  let worker
  let disposed = false
  workerButton.addEventListener('click', async () => {
    try {
      const configResponse = await fetch(new URL('./dist/assets/message.json', import.meta.url))
      const workerResponse = await fetch(new URL('./dist/workers/echo.js', import.meta.url))
      if (!configResponse.ok || !workerResponse.ok) throw new Error('Resource unavailable')
      const config = await configResponse.json()
      const source = await workerResponse.text()
      if (disposed) return
      worker?.terminate()
      if (workerUrl) URL.revokeObjectURL(workerUrl)
      const url = URL.createObjectURL(new Blob([source], { type: 'text/javascript' }))
      workerUrl = url
      worker = new Worker(url)
      worker.onmessage = ({ data }) => {
        if (!disposed) output.textContent = JSON.stringify(data)
      }
      worker.onerror = (event) => {
        if (!disposed) output.textContent = 'Worker failed: ' + event.message
      }
      worker.postMessage(config.message)
    } catch (error) {
      if (!disposed) output.textContent = String(error)
    }
  })
  button.addEventListener('click', async () => {
    try {
      output.textContent = JSON.stringify(await client.ping())
    } catch (error) {
      output.textContent = String(error)
    }
  })
  root.replaceChildren(button, workerButton, exportButton, output)
  return () => {
    disposed = true
    if (exportUrl) URL.revokeObjectURL(exportUrl)
    worker?.terminate()
    if (workerUrl) URL.revokeObjectURL(workerUrl)
    root.replaceChildren()
  }
}
