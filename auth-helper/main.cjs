'use strict';

delete process.env.DEBUG;
for (const method of ['log', 'info', 'warn', 'error', 'debug']) console[method] = () => {};
const { Authflow } = require('prismarine-auth');
const { authenticate, errorCode } = require('./auth.cjs');
const readline = require('node:readline');
const originalFetch = globalThis.fetch;
globalThis.fetch = (url, options = {}) => originalFetch(url, {
  ...options,
  signal: options.signal ? AbortSignal.any([options.signal, AbortSignal.timeout(30000)]) : AbortSignal.timeout(30000),
});
const emit = value => process.stdout.write(JSON.stringify(value) + '\n');
const finish = (value, status) => process.stdout.write(JSON.stringify(value) + '\n', () => process.exit(status));
const lines = readline.createInterface({ input: process.stdin });
// Parent death closes stdin, so no orphan keeps polling Microsoft.
lines.on('close', () => process.exit(1));
lines.once('line', async line => {
  try {
    if (line.length > 4 * 1024 * 1024) throw new Error('INVALID_REQUEST');
    const account = await authenticate(JSON.parse(line), emit, Authflow);
    finish({ type: 'account', account }, 0);
  } catch (error) {
    finish({ type: 'error', code: errorCode(error) }, 1);
  }
});
