import test from 'node:test';
import assert from 'node:assert/strict';
import worker, { packageTarget } from '../src/index.js';

const R = 'https://github.com/vicrodh/chilpa/releases/download';
const env = { ASSETS: { fetch: async () => new Response('asset', { status: 200 }) } };
const get = (path, method = 'GET') => worker.fetch(new Request(`https://pkgs.chilpa.app${path}`, { method }), env);

test('deb paths map to the release asset', () => {
  assert.equal(packageTarget('/apt/pool/main/c/chilpa/chilpa_0.1.0_amd64.deb'), `${R}/v0.1.0/chilpa_0.1.0_amd64.deb`);
  assert.equal(packageTarget('/apt/pool/main/c/chilpa/chilpa_12.3.45_arm64.deb'), `${R}/v12.3.45/chilpa_12.3.45_arm64.deb`);
});

test('rpm paths map to the release asset', () => {
  assert.equal(packageTarget('/dnf/x86_64/Packages/chilpa-0.1.0-1.x86_64.rpm'), `${R}/v0.1.0/chilpa-0.1.0-1.x86_64.rpm`);
  assert.equal(packageTarget('/dnf/aarch64/Packages/chilpa-0.2.0-1.aarch64.rpm'), `${R}/v0.2.0/chilpa-0.2.0-1.aarch64.rpm`);
});

test('anything that is not a strict package path is not redirected', () => {
  for (const p of [
    '/apt/pool/main/c/chilpa/other_0.1.0_amd64.deb',
    '/apt/pool/main/c/chilpa/chilpa_0.1.0_i386.deb',
    '/apt/pool/main/c/chilpa/../chilpa_0.1.0_amd64.deb',
    '/apt/pool/main/c/chilpa/chilpa_0.1.0-rc1_amd64.deb',
    '/dnf/x86_64/Packages/chilpa-0.1.0-1.aarch64.rpm',
    '/dnf/ppc64/Packages/chilpa-0.1.0-1.ppc64.rpm',
    '/apt/dists/stable/Release',
  ]) assert.equal(packageTarget(p), null, p);
});

test('a package path answers 302 with the release location', async () => {
  const res = await get('/apt/pool/main/c/chilpa/chilpa_0.1.0_amd64.deb');
  assert.equal(res.status, 302);
  assert.equal(res.headers.get('location'), `${R}/v0.1.0/chilpa_0.1.0_amd64.deb`);
  const head = await get('/dnf/x86_64/Packages/chilpa-0.1.0-1.x86_64.rpm', 'HEAD');
  assert.equal(head.status, 302);
});

test('a malformed package path is a 404, not a static lookup', async () => {
  assert.equal((await get('/apt/pool/main/c/chilpa/evil.deb')).status, 404);
  assert.equal((await get('/dnf/x86_64/Packages/evil.rpm')).status, 404);
});

test('index files come from the static assets', async () => {
  const res = await get('/apt/dists/stable/InRelease');
  assert.equal(res.status, 200);
  assert.equal(await res.text(), 'asset');
});

test('writes are refused', async () => {
  assert.equal((await get('/chilpa.asc', 'POST')).status, 405);
});
