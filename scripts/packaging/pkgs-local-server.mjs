// Serves a built pkgs.chilpa.app site locally through the REAL Worker source.
// usage: node pkgs-local-server.mjs <site-dir> <port>
// Package paths answer with the Worker's 302 to the GitHub release asset.
import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { join, normalize } from 'node:path';
import worker from '../../packaging/pkgs-worker/src/index.js';

const [site, port] = process.argv.slice(2);
if (!site || !port) throw new Error('usage: pkgs-local-server.mjs <site-dir> <port>');

const env = {
  ASSETS: {
    async fetch(request) {
      const { pathname } = new URL(request.url);
      const rel = normalize(decodeURIComponent(pathname)).replace(/^(\.\.[/\\])+/, '');
      try {
        return new Response(await readFile(join(site, rel)), { status: 200 });
      } catch {
        return new Response('Not found', { status: 404 });
      }
    },
  },
};

http.createServer(async (req, res) => {
  const response = await worker.fetch(new Request(`http://127.0.0.1:${port}${req.url}`, { method: req.method }), env);
  res.writeHead(response.status, Object.fromEntries(response.headers));
  res.end(req.method === 'HEAD' ? undefined : Buffer.from(await response.arrayBuffer()));
  console.error(`${req.method} ${req.url} -> ${response.status}`);
}).listen(Number(port), '127.0.0.1');
