// A local registry for the unmodified npm tarballs under test. Serving metadata
// for every platform lets npm perform its real OS/CPU/libc dependency selection.
// A worker keeps the server responsive while the parent waits for npm commands.
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { parentPort, workerData } from 'node:worker_threads';

const { packages, packed, scope } = workerData;
let base;
const server = createServer((request, response) => {
	try {
		const path = decodeURIComponent(new URL(request.url, base).pathname).slice(1);
		if (path.startsWith('-/')) {
			const name = path.slice(2);
			if (!packed[name]) {
				response.writeHead(404).end('No tarball for this runtime target');
				return;
			}
			response.writeHead(200, { 'content-type': 'application/octet-stream' }).end(readFileSync(packed[name]));
			return;
		}
		const pkg = packages[path];
		if (!pkg) {
			if (path.startsWith(`${scope}/`)) response.writeHead(404).end('Package absent from artifact');
			else response.writeHead(302, { location: `https://registry.npmjs.org/${encodeURIComponent(path)}` }).end();
			return;
		}
		const dist = { tarball: `${base}/-/${encodeURIComponent(pkg.name)}` };
		if (packed[pkg.name]) dist.integrity = `sha512-${createHash('sha512').update(readFileSync(packed[pkg.name])).digest('base64')}`;
		response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({
			name: pkg.name,
			'dist-tags': { latest: pkg.version },
			versions: { [pkg.version]: { ...pkg, dist } },
		}));
	} catch (error) {
		response.writeHead(500).end(error.message);
	}
});
server.listen(0, '127.0.0.1', () => {
	base = `http://127.0.0.1:${server.address().port}`;
	parentPort.postMessage(base);
});
