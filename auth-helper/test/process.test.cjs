const { test } = require('node:test');
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const path = require('node:path');

test('bundled helper reports only structured errors and exits on malformed input', async () => {
  const child = spawn(process.execPath, [path.join(__dirname, '../dist/main.cjs')], { env: { ...process.env, DEBUG: '*' } });
  let output = '', errors = '';
  child.stdout.on('data', data => output += data);
  child.stderr.on('data', data => errors += data);
  child.stdin.write('{"mode":"invalid","cache":{"token":"do-not-log"}}\n');
  const timer = setTimeout(() => child.kill(), 5000);
  const status = await new Promise(resolve => child.on('close', resolve));
  clearTimeout(timer);
  assert.equal(status, 1);
  assert.deepEqual(JSON.parse(output), { type: 'error', code: 'INVALID_REQUEST' });
  assert.equal(errors, '');
});

test('bundled helper exits when the parent closes its input', async () => {
  const child = spawn(process.execPath, [path.join(__dirname, '../dist/main.cjs')]);
  child.stdin.end();
  const timer = setTimeout(() => child.kill(), 5000);
  const status = await new Promise(resolve => child.on('close', resolve));
  clearTimeout(timer);
  assert.equal(status, 1);
});
