import { createClient, type Request } from '@cruciblebox/next-api'
const client = createClient({
  session: 'A'.repeat(32),
  exchange: async (request: Request) => ({
    wireVersion: 3,
    requestId: request.requestId,
    ok: true,
    result: null
  })
})
void client.storage.set('note.v1', { text: 'preserved' })
// @ts-expect-error A storage set requires a value.
void client.storage.set('note.v1')
// @ts-expect-error No host SQL capability is exported.
void client.db.query('SELECT * FROM settings')
