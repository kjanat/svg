import { defineConfig } from 'importmapify';
import { readFileSync } from 'node:fs';

/** bun.lock is JSONC (trailing commas). Do not JSON.parse it as JSON. */
function loadBunLock() {
	const text = readFileSync(new URL('./bun.lock', import.meta.url), 'utf8');
	return JSON.parse(text.replace(/,\s*([}\]])/g, '$1'));
}

/** Resolved version of `name` from bun.lock `packages`. */
function lockedVersion(lock, name) {
	const entry = lock.packages?.[name];
	if (!Array.isArray(entry) || typeof entry[0] !== 'string') {
		throw new Error(`bun.lock has no resolved package ${name}`);
	}
	const id = entry[0];
	const at = id.lastIndexOf('@');
	if (at <= 0) throw new Error(`bun.lock entry ${name} has no version: ${id}`);
	return id.slice(at + 1);
}

const lock = loadBunLock();
const dreamcli = lockedVersion(lock, '@kjanat/dreamcli');
const bunTypes = lockedVersion(lock, 'bun-types');
const bcd = lockedVersion(lock, '@mdn/browser-compat-data');
const webFeatures = lockedVersion(lock, 'web-features');
const preact = lockedVersion(lock, 'preact');
const preactRts = lockedVersion(lock, 'preact-render-to-string');

export default defineConfig({
	out: 'import_map.json',
	extensions: ['ts', 'tsx', 'js', 'mjs'],
	packages: {
		'@std/assert': 'jsr:@std/assert@1',
		'@std/html': 'jsr:@std/html@1',
		'@std/http': 'jsr:@std/http@1',
		'@std/media-types': 'jsr:@std/media-types@1',
		'@std/path': 'jsr:@std/path@1',
		bcd: `npm:@mdn/browser-compat-data@${bcd}`,
		'bun-types': `npm:bun-types@${bunTypes}`,
		dreamcli: `jsr:@kjanat/dreamcli@${dreamcli}`,
		preact: `https://esm.sh/preact@${preact}`,
		'preact-render-to-string': `https://esm.sh/preact-render-to-string@${preactRts}`,
		react: `https://esm.sh/preact@${preact}/compat`,
		'react-dom': `https://esm.sh/preact@${preact}/compat`,
		'web-features': `npm:web-features@${webFeatures}`,
	},
});
