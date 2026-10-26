'use strict';

// Rust owns persistence. No prismarine-auth cache is ever written to disk here.
function memoryCache(seed = {}) {
  const data = structuredClone(seed);
  const factory = ({ cacheName }) => ({
    async getCached() { return data[cacheName] ??= {}; },
    async setCached(value) { data[cacheName] = value; },
    async setCachedPartial(value) { data[cacheName] = { ...data[cacheName], ...value }; },
    async reset() { data[cacheName] = {}; },
  });
  return { data, factory };
}

function deviceCode(response) {
  const url = new URL(response.verification_uri);
  if (url.protocol !== 'https:' || !['microsoft.com', 'www.microsoft.com', 'login.microsoftonline.com'].includes(url.hostname)
      || url.username || url.password || url.port) throw new Error('INVALID_DEVICE_CODE');
  if (!/^[A-Z0-9-]{4,20}$/.test(response.user_code) || !(response.expires_in > 0)) throw new Error('INVALID_DEVICE_CODE');
  return { type: 'device_code', user_code: response.user_code, verification_uri: url.href, expires_in: response.expires_in };
}

function accountFrom(result, cache, now = Date.now()) {
  const profile = result.profile;
  if (!profile || !/^[a-f0-9]{32}$/i.test(profile.id) || !/^[A-Za-z0-9_]{1,16}$/.test(profile.name)) {
    throw new Error('PROFILE_UNAVAILABLE');
  }
  const token = cache.mca?.mca;
  if (typeof result.token !== 'string' || !result.token || token?.access_token !== result.token) throw new Error('INVALID_TOKEN');
  const expires = Math.floor((token.obtainedOn + token.expires_in * 1000) / 1000);
  if (!Number.isSafeInteger(expires) || expires * 1000 <= now + 60000) throw new Error('INVALID_TOKEN');
  let xuid = '0';
  try {
    const claims = JSON.parse(Buffer.from(result.token.split('.')[1], 'base64url'));
    if (/^\d+$/.test(String(claims.xuid))) xuid = String(claims.xuid);
  } catch {}
  let skin = null;
  try {
    const active = profile.skins?.find(s => s.state === 'ACTIVE');
    const url = new URL(active?.url);
    if (url.hostname === 'textures.minecraft.net' && ['http:', 'https:'].includes(url.protocol)) {
      url.protocol = 'https:';
      skin = url.href;
    }
  } catch {}
  return { name: profile.name, uuid: profile.id, xuid, mc_token: result.token, mc_expires_at: expires, skin_url: skin, cache };
}

async function authenticate(request, emit, Authflow) {
  if (!['login', 'refresh'].includes(request.mode)) throw new Error('INVALID_REQUEST');
  const cache = memoryCache(request.mode === 'refresh' ? request.cache : {});
  // A nearly expired cached MC token is unsuitable for starting the game.
  const mc = cache.data.mca?.mca;
  if (mc && mc.obtainedOn + mc.expires_in * 1000 < Date.now() + 600000) delete cache.data.mca.mca;
  // Use prismarine-auth's documented default live flow and title.
  const flow = new Authflow('ember', cache.factory, undefined, response => {
    // URIError prevents the library's retry loop from requesting more codes.
    if (request.mode === 'refresh') throw new URIError('SIGN_IN_REQUIRED');
    emit(deviceCode(response));
  });
  const result = await flow.getMinecraftJavaToken({ fetchProfile: true });
  return accountFrom(result, cache.data);
}

function errorCode(error) {
  const message = String(error?.message ?? '');
  for (const code of ['SIGN_IN_REQUIRED', 'PROFILE_UNAVAILABLE', 'INVALID_TOKEN', 'INVALID_DEVICE_CODE', 'INVALID_REQUEST']) {
    if (message.includes(code)) return code;
  }
  for (const code of ['2148916233', '2148916235', '2148916236', '2148916237', '2148916238']) {
    if (message.includes(code)) return code;
  }
  if (/timed? ?out|expired_token/i.test(message)) return 'TIMEOUT';
  if (/fetch|network|ENOTFOUND|ECONN/i.test(message)) return 'NETWORK';
  if (/403/.test(message)) return 'FORBIDDEN';
  return 'AUTH_FAILED'; // Never forward upstream response bodies or tokens.
}

module.exports = { memoryCache, deviceCode, accountFrom, authenticate, errorCode };
