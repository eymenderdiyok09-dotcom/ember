const { test } = require('node:test');
const assert = require('node:assert/strict');
const { memoryCache, deviceCode, accountFrom, authenticate, errorCode } = require('../auth.cjs');

const now = Date.now();
const token = 'header.' + Buffer.from(JSON.stringify({ xuid: '2535400000000000' })).toString('base64url') + '.signature';
const profile = { id: '0123456789abcdef0123456789abcdef', name: 'Player', skins: [{ state: 'ACTIVE', url: 'http://textures.minecraft.net/texture/example' }] };
const cache = () => ({ mca: { mca: { access_token: token, obtainedOn: now, expires_in: 86400 } }, live: { token: { refresh_token: 'private' } } });
const code = { user_code: 'EMBR-7Q2K', verification_uri: 'https://www.microsoft.com/link', expires_in: 900, device_code: 'secret-device-code' };

test('memory cache merges namespaces and does not mutate its input', async () => {
  const seed = cache();
  const c = memoryCache(seed);
  await c.factory({ cacheName: 'live' }).setCachedPartial({ extra: true });
  assert.equal(c.data.live.token.refresh_token, 'private');
  assert.equal(c.data.live.extra, true);
  assert.equal(seed.live.extra, undefined);
  await c.factory({ cacheName: 'live' }).reset();
  assert.deepEqual(c.data.live, {});
  assert.ok(c.data.mca);
});

test('only public device-code fields can be emitted', () => {
  assert.deepEqual(deviceCode(code), { type: 'device_code', user_code: 'EMBR-7Q2K', verification_uri: code.verification_uri, expires_in: 900 });
  for (const url of ['http://microsoft.com/link', 'https://microsoft.com.attacker.test/', 'https://user@microsoft.com/link']) {
    assert.throws(() => deviceCode({ ...code, verification_uri: url }));
  }
});

test('account maps name, UUID, XUID, expiry and active skin; rejects missing profile and expired token', () => {
  const account = accountFrom({ token, profile }, cache(), now);
  assert.equal(account.xuid, '2535400000000000');
  assert.equal(account.uuid, profile.id);
  assert.equal(account.skin_url, 'https://textures.minecraft.net/texture/example');
  assert.equal(account.mc_expires_at, Math.floor(now / 1000) + 86400);
  assert.throws(() => accountFrom({ token }, cache(), now), /PROFILE_UNAVAILABLE/);
  assert.throws(() => accountFrom({ token, profile }, cache(), now + 86400000), /INVALID_TOKEN/);
});

test('login starts fresh and emits a code; refresh reuses caches without showing a code', async () => {
  const events = [];
  class FakeFlow {
    constructor(username, factory, options, callback) {
      assert.equal(username, 'ember');
      assert.equal(options, undefined);
      this.factory = factory;
      this.callback = callback;
    }
    async getMinecraftJavaToken(options) {
      assert.equal(options.fetchProfile, true);
      const c = this.factory({ cacheName: 'mca' });
      if (!(await c.getCached()).mca) {
        this.callback(code);
        await c.setCached(cache().mca);
      }
      return { token, profile };
    }
  }
  const login = await authenticate({ mode: 'login', cache: cache() }, e => events.push(e), FakeFlow);
  assert.equal(events.length, 1);
  assert.equal(login.name, 'Player');
  events.length = 0;
  await authenticate({ mode: 'refresh', cache: cache() }, e => events.push(e), FakeFlow);
  assert.equal(events.length, 0);
  await assert.rejects(authenticate({ mode: 'refresh', cache: {} }, e => events.push(e), FakeFlow), /SIGN_IN_REQUIRED/);
  assert.equal(events.length, 0);
});

test('near-expiry MC token is renewed before starting Minecraft', async () => {
  const seed = cache();
  seed.mca.mca.expires_in = 60;
  class RenewFlow {
    constructor(_name, factory) { this.cache = factory({ cacheName: 'mca' }); }
    async getMinecraftJavaToken() {
      assert.equal((await this.cache.getCached()).mca, undefined);
      await this.cache.setCached(cache().mca);
      return { token, profile };
    }
  }
  await authenticate({ mode: 'refresh', cache: seed }, () => assert.fail('Unexpected device code'), RenewFlow);
});

test('real prismarine-auth accepts the custom cache and fetches the Java profile', async () => {
  const original = global.fetch;
  const calls = [];
  global.fetch = async (url, options) => {
    calls.push(String(url));
    assert.equal(String(url), 'https://api.minecraftservices.com/minecraft/profile');
    assert.equal(options.headers.Authorization, 'Bearer ' + token);
    return new Response(JSON.stringify(profile), { status: 200, headers: { 'content-type': 'application/json' } });
  };
  try {
    const { Authflow } = require('prismarine-auth');
    const result = await authenticate({ mode: 'refresh', cache: cache() }, () => assert.fail('Unexpected device code'), Authflow);
    assert.equal(result.name, 'Player');
    assert.equal(calls.length, 1);
  } finally { global.fetch = original; }
});

test('errors never forward upstream token-bearing details', () => {
  assert.equal(errorCode(new Error('server response: access_token=secret')), 'AUTH_FAILED');
  assert.equal(errorCode(new Error('XErr 2148916238')), '2148916238');
  assert.equal(errorCode(new Error('fetch failed')), 'NETWORK');
});
