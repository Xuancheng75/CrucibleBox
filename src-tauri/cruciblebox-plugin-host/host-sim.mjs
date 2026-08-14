// 宿主模拟器：spawn sidecar，走帧协议驱动 lifecycle.initialize → plugin.message → dispose
// 并响应 sidecar 发出的 host 方法请求（log.write / storage.* 等）。
// 用法: node host-sim.mjs <sidecarExe> <pluginDir> <mainEntry> <token>
import { spawn } from 'node:child_process';

const [exe, pluginDir, mainEntry, token] = process.argv.slice(2);
if (!exe || !pluginDir || !mainEntry || !token) {
  console.error('usage: node host-sim.mjs <exe> <pluginDir> <mainEntry> <token>');
  process.exit(1);
}

const child = spawn(exe, [pluginDir, mainEntry, '2', token], { stdio: ['pipe', 'pipe', 'inherit'] });
const stdin = child.stdin;
const stdout = child.stdout;

let buffer = Buffer.alloc(0);
let pendingResolve = null;

// sidecar 发出的 host 方法响应（内存 storage）
const hostStorage = new Map([['counter', 42]]);

function handleHostRequest(method, params, requestId) {
  const result = (() => {
    if (method === 'log.write') return null;
    if (method === 'storage.get') return hostStorage.has(params.key) ? hostStorage.get(params.key) : null;
    if (method === 'storage.set') { hostStorage.set(params.key, params.value); return null; }
    if (method === 'db.query') return [];
    if (method === 'db.execute') return null;
    if (method === 'event.subscribe' || method === 'event.unsubscribe') return null;
    if (method === 'event.emit' || method === 'notification.show') return null;
    throw new Error('unhandled host method: ' + method);
  })();
  send({
    v: 2, kind: 'response', requestId, ok: true, result,
  });
}

function dispatch(envelope) {
  if (envelope.kind === 'request') {
    // sidecar 的 host 方法请求（lifecycle.*/plugin.message 是宿主发的，sidecar 不回发）
    handleHostRequest(envelope.method, envelope.params, envelope.requestId);
  } else if (envelope.kind === 'response') {
    if (pendingResolve) {
      const resolve = pendingResolve;
      pendingResolve = null;
      resolve(envelope);
    } else {
      console.warn('[sim] orphan response:', JSON.stringify(envelope));
    }
  } else {
    console.warn('[sim] unknown kind:', envelope.kind);
  }
}

stdout.on('data', (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  while (buffer.length >= 4) {
    const len = buffer.readUInt32BE(0);
    if (buffer.length < 4 + len) break;
    const payload = buffer.subarray(4, 4 + len).toString('utf8');
    buffer = buffer.subarray(4 + len);
    dispatch(JSON.parse(payload));
  }
});

function send(envelope) {
  const bytes = Buffer.from(JSON.stringify(envelope), 'utf8');
  const header = Buffer.alloc(4);
  header.writeUInt32BE(bytes.length, 0);
  stdin.write(Buffer.concat([header, bytes]));
}

function request(requestId, method, params) {
  send({ v: 2, kind: 'request', token, requestId, method, params });
  return new Promise((resolve) => { pendingResolve = resolve; });
}

let requestCounter = 0;
const rid = () => `req-${++requestCounter}`;

const results = [];
const failures = [];

async function run() {
  const initRes = await request(rid(), 'lifecycle.initialize', { id: 'sim-plugin', config: {} });
  results.push(['initialize', initRes]);
  if (initRes.ok !== true) failures.push('initialize failed: ' + JSON.stringify(initRes));

  const msgRes = await request(rid(), 'plugin.message', { message: { type: 'ping' } });
  results.push(['message', msgRes]);

  const disposeRes = await request(rid(), 'lifecycle.dispose', {});
  results.push(['dispose', disposeRes]);
  if (disposeRes.ok !== true) failures.push('dispose failed: ' + JSON.stringify(disposeRes));

  child.kill();
  console.log('=== RESULTS ===');
  for (const [name, res] of results) {
    console.log(name, '->', JSON.stringify(res));
  }
  if (failures.length) {
    console.error('FAILURES:', failures.join('; '));
    process.exit(1);
  }
  console.log('=== E2E PASSED ===');
  process.exit(0);
}

setTimeout(() => {
  console.error('TIMEOUT waiting for sidecar');
  child.kill();
  process.exit(1);
}, 30000);

run();
