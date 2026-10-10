import { createClient } from '@cruciblebox/next-api'
export async function mount({ root, session, exchange }) {
  const client = createClient({ session, exchange })
  const input = document.createElement('textarea')
  const saved = await client.backend.call('load')
  input.value = saved?.text ?? ''
  const button = document.createElement('button')
  button.textContent = 'Save note'
  const status = document.createElement('p')
  button.addEventListener('click', async () => {
    button.disabled = true
    try {
      await client.backend.call('save', [input.value])
      status.textContent = 'Saved'
    } catch (error) {
      status.textContent = String(error)
    } finally {
      button.disabled = false
    }
  })
  root.replaceChildren(input, button, status)
  return () => root.replaceChildren()
}
