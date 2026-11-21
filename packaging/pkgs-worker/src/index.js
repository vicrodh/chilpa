// pkgs.chilpa.app: static APT and DNF indices plus redirects for the packages,
// and the counted entry points of the Flatpak repository.
//
// The packages live in GitHub releases and are never copied here. Index files
// come from the static assets (built by scripts/packaging/build-pkgs-repo.sh);
// every package path is answered with a 302 to the release asset.
//
// The Flatpak repository is served by flatpak.chilpa.app, which reports no
// visits. Its two descriptors are reached through here: each GET is counted in
// D1 by day, file, client and country, keeping no address and no user agent,
// and answered with a 302 to the descriptor itself. /stats/flatpak.json
// returns the counts.

const RELEASES = 'https://github.com/vicrodh/chilpa/releases/download';
const FLATPAK_SITE = 'https://flatpak.chilpa.app';
const FLATPAK_FILES = new Set(['app.chilpa.Chilpa.flatpakref', 'chilpa.flatpakrepo']);

// /apt/pool/main/c/chilpa/chilpa_0.1.0_amd64.deb
const DEB = /^\/apt\/pool\/main\/c\/chilpa\/(chilpa_(\d+\.\d+\.\d+)_(?:amd64|arm64)\.deb)$/;
// /dnf/x86_64/Packages/chilpa-0.1.0-1.x86_64.rpm
const RPM = /^\/dnf\/(x86_64|aarch64)\/Packages\/(chilpa-(\d+\.\d+\.\d+)-[0-9]+\.(x86_64|aarch64)\.rpm)$/;

const SCHEMA = `CREATE TABLE IF NOT EXISTS flatpak_requests (
  day TEXT NOT NULL,
  file TEXT NOT NULL,
  client TEXT NOT NULL,
  country TEXT NOT NULL,
  n INTEGER NOT NULL,
  PRIMARY KEY (day, file, client, country)
)`;
const COUNT = `INSERT INTO flatpak_requests (day, file, client, country, n) VALUES (?1, ?2, ?3, ?4, 1)
  ON CONFLICT (day, file, client, country) DO UPDATE SET n = n + 1`;
const STATS = 'SELECT day, file, client, country, n FROM flatpak_requests ORDER BY day, file, client, country';

/** Returns the release asset URL for a package path, or null when it is not one. */
export function packageTarget(pathname) {
  let m = DEB.exec(pathname);
  if (m) return `${RELEASES}/v${m[2]}/${m[1]}`;
  m = RPM.exec(pathname);
  if (m && m[1] === m[4]) return `${RELEASES}/v${m[3]}/${m[2]}`;
  return null;
}

/** Returns the Flatpak descriptor a path names, or null when it names none. */
export function flatpakFile(pathname) {
  const m = /^\/flatpak\/([^/]+)$/.exec(pathname);
  return m && FLATPAK_FILES.has(m[1]) ? m[1] : null;
}

/** flatpak, browser or other, read from the User-Agent header. */
export function clientKind(userAgent) {
  const ua = userAgent || '';
  if (/^flatpak[ /]/i.test(ua)) return 'flatpak';
  if (ua.includes('Mozilla/')) return 'browser';
  return 'other';
}

/** The visitor's two-letter country as Cloudflare reports it, or XX. */
export function countryCode(cf) {
  const c = cf && typeof cf.country === 'string' ? cf.country.toUpperCase() : '';
  return /^[A-Z0-9]{2}$/.test(c) ? c : 'XX';
}

/** Adds one request to its day, file, client and country. */
export async function countFlatpak(db, { day, file, client, country }) {
  await db.batch([db.prepare(SCHEMA), db.prepare(COUNT).bind(day, file, client, country)]);
}

async function flatpakStats(db) {
  try {
    const { results } = await db.prepare(STATS).all();
    return results;
  } catch (e) {
    if (/no such table/i.test(String(e && e.message))) return [];
    throw e;
  }
}

function json(body, status, cache) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': cache, 'Access-Control-Allow-Origin': '*' },
  });
}

export default {
  async fetch(request, env, ctx) {
    if (request.method !== 'GET' && request.method !== 'HEAD') {
      return new Response('Method not allowed', { status: 405, headers: { Allow: 'GET, HEAD' } });
    }
    const { pathname } = new URL(request.url);
    const target = packageTarget(pathname);
    if (target) {
      return new Response(null, { status: 302, headers: { Location: target, 'Cache-Control': 'public, max-age=300' } });
    }
    if (pathname.includes('/pool/') || pathname.endsWith('.deb') || pathname.endsWith('.rpm')) {
      return new Response('Not found', { status: 404 });
    }
    const file = flatpakFile(pathname);
    if (file) {
      // The redirect never waits on the count, and a failed count does not stop it.
      if (request.method === 'GET' && env.STATS && ctx) {
        const visit = {
          day: new Date().toISOString().slice(0, 10),
          file,
          client: clientKind(request.headers.get('User-Agent')),
          country: countryCode(request.cf),
        };
        ctx.waitUntil(countFlatpak(env.STATS, visit).catch((e) => console.error('flatpak count failed:', e)));
      }
      return new Response(null, { status: 302, headers: { Location: `${FLATPAK_SITE}/${file}`, 'Cache-Control': 'no-store' } });
    }
    if (pathname.startsWith('/flatpak/')) {
      return new Response('Not found', { status: 404 });
    }
    if (pathname === '/stats/flatpak.json') {
      if (!env.STATS) return json({ error: 'not configured' }, 503, 'no-store');
      return json({ rows: await flatpakStats(env.STATS) }, 200, 'public, max-age=300');
    }
    return env.ASSETS.fetch(request);
  },
};
