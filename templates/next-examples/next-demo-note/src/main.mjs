import { createClient } from '@cruciblebox/next-api'
export function activate({ session, exchange }) {
  const client = createClient({ session, exchange })
  return {
    load: () => client.storage.get('note.v1'),
    save: (note) => client.storage.set('note.v1', { schema: 1, text: note })
  }
}
