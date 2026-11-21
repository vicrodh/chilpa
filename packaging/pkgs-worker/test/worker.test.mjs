import test from 'node:test';
import assert from 'node:assert/strict';
import worker, { clientKind, countryCode, flatpakFile, packageTarget } from '../src/index.js';

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

// A D1 stand-in: records every batch and answers the stats query with `rows`.
function fakeDb({ rows = [], failBatch = false, missingTable = false } = {}) {
  const batches = [];
  const statement = (sql) => ({
    sql,
    args: [],
    bind(...args) { this.args = args; return this; },
    async all() {
      if (missingTable) throw new Error('D1_ERROR: no such table: flatpak_requests');
      return { results: rows };
    },
  });
  return {
    batches,
    prepare: statement,
    async batch(statements) {
      if (failBatch) throw new Error('D1 is down');
      batches.push(statements.map((s) => ({ sql: s.sql, args: s.args })));
      return [];
    },
  };
}

// Runs one request with a waitUntil that is awaited before returning.
async function visit(path, { method = 'GET', ua, country, db = fakeDb(), stats = true } = {}) {
  const headers = ua ? { 'User-Agent': ua } : {};
  const request = new Request(`https://pkgs.chilpa.app${path}`, { method, headers });
  if (country !== undefined) Object.defineProperty(request, 'cf', { value: { country } });
  const pending = [];
  const res = await worker.fetch(request, { ...env, ...(stats ? { STATS: db } : {}) }, { waitUntil: (p) => pending.push(p) });
  await Promise.all(pending);
  return { res, db };
}

test('only the two Flatpak descriptors are counted entry points', () => {
  assert.equal(flatpakFile('/flatpak/app.chilpa.Chilpa.flatpakref'), 'app.chilpa.Chilpa.flatpakref');
  assert.equal(flatpakFile('/flatpak/chilpa.flatpakrepo'), 'chilpa.flatpakrepo');
  for (const p of [
    '/flatpak/',
    '/flatpak/other.flatpakref',
    '/flatpak/../chilpa.flatpakrepo',
    '/flatpak/chilpa.flatpakrepo/x',
    '/chilpa.flatpakrepo',
    '/flatpak/repo/summary',
  ]) assert.equal(flatpakFile(p), null, p);
});

test('the client is told apart by its user agent', () => {
  assert.equal(clientKind('Flatpak 1.18.4'), 'flatpak');
  assert.equal(clientKind('flatpak/1.14.10'), 'flatpak');
  assert.equal(clientKind('Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0'), 'browser');
  assert.equal(clientKind('curl/8.11.0'), 'other');
  assert.equal(clientKind(null), 'other');
  assert.equal(clientKind('Flatpakker'), 'other');
});

test('the country is two letters or XX', () => {
  assert.equal(countryCode({ country: 'MX' }), 'MX');
  assert.equal(countryCode({ country: 't1' }), 'T1');
  assert.equal(countryCode({ country: 'Mexico' }), 'XX');
  assert.equal(countryCode({}), 'XX');
  assert.equal(countryCode(undefined), 'XX');
});

test('a descriptor request is counted and redirected to the repository', async () => {
  const { res, db } = await visit('/flatpak/app.chilpa.Chilpa.flatpakref', { ua: 'Flatpak 1.18.4', country: 'MX' });
  assert.equal(res.status, 302);
  assert.equal(res.headers.get('location'), 'https://flatpak.chilpa.app/app.chilpa.Chilpa.flatpakref');
  assert.equal(res.headers.get('cache-control'), 'no-store');
  assert.equal(db.batches.length, 1);
  const [schema, count] = db.batches[0];
  assert.match(schema.sql, /^CREATE TABLE IF NOT EXISTS flatpak_requests/);
  assert.match(count.sql, /ON CONFLICT \(day, file, client, country\) DO UPDATE SET n = n \+ 1/);
  const [day, file, client, country] = count.args;
  assert.match(day, /^\d{4}-\d{2}-\d{2}$/);
  assert.deepEqual([file, client, country], ['app.chilpa.Chilpa.flatpakref', 'flatpak', 'MX']);
});

test('a HEAD request is redirected and not counted', async () => {
  const { res, db } = await visit('/flatpak/chilpa.flatpakrepo', { method: 'HEAD', ua: 'Flatpak 1.18.4' });
  assert.equal(res.status, 302);
  assert.equal(res.headers.get('location'), 'https://flatpak.chilpa.app/chilpa.flatpakrepo');
  assert.equal(db.batches.length, 0);
});

test('the redirect still happens when the count fails or has no database', async () => {
  const failing = await visit('/flatpak/chilpa.flatpakrepo', { db: fakeDb({ failBatch: true }) });
  assert.equal(failing.res.status, 302);
  const unbound = await visit('/flatpak/chilpa.flatpakrepo', { stats: false });
  assert.equal(unbound.res.status, 302);
});

test('any other path under /flatpak/ is a 404, not a static lookup', async () => {
  const { res, db } = await visit('/flatpak/repo/summary');
  assert.equal(res.status, 404);
  assert.equal(db.batches.length, 0);
});

test('the stats are the stored rows, public and cached briefly', async () => {
  const rows = [{ day: '2026-11-21', file: 'app.chilpa.Chilpa.flatpakref', client: 'flatpak', country: 'MX', n: 3 }];
  const { res } = await visit('/stats/flatpak.json', { db: fakeDb({ rows }) });
  assert.equal(res.status, 200);
  assert.equal(res.headers.get('cache-control'), 'public, max-age=300');
  assert.equal(res.headers.get('access-control-allow-origin'), '*');
  assert.deepEqual(await res.json(), { rows });
});

test('the stats are empty before the first count creates the table', async () => {
  const { res } = await visit('/stats/flatpak.json', { db: fakeDb({ missingTable: true }) });
  assert.equal(res.status, 200);
  assert.deepEqual(await res.json(), { rows: [] });
});

test('the stats say so when the database is not bound', async () => {
  const { res } = await visit('/stats/flatpak.json', { stats: false });
  assert.equal(res.status, 503);
});
