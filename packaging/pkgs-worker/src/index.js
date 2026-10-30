// pkgs.chilpa.app: static APT and DNF indices plus redirects for the packages.
//
// The packages live in GitHub releases and are never copied here. Index files
// come from the static assets (built by scripts/packaging/build-pkgs-repo.sh);
// every package path is answered with a 302 to the release asset.

const RELEASES = 'https://github.com/vicrodh/chilpa/releases/download';

// /apt/pool/main/c/chilpa/chilpa_0.1.0_amd64.deb
const DEB = /^\/apt\/pool\/main\/c\/chilpa\/(chilpa_(\d+\.\d+\.\d+)_(?:amd64|arm64)\.deb)$/;
// /dnf/x86_64/Packages/chilpa-0.1.0-1.x86_64.rpm
const RPM = /^\/dnf\/(x86_64|aarch64)\/Packages\/(chilpa-(\d+\.\d+\.\d+)-[0-9]+\.(x86_64|aarch64)\.rpm)$/;

/** Returns the release asset URL for a package path, or null when it is not one. */
export function packageTarget(pathname) {
  let m = DEB.exec(pathname);
  if (m) return `${RELEASES}/v${m[2]}/${m[1]}`;
  m = RPM.exec(pathname);
  if (m && m[1] === m[4]) return `${RELEASES}/v${m[3]}/${m[2]}`;
  return null;
}

export default {
  async fetch(request, env) {
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
    return env.ASSETS.fetch(request);
  },
};
