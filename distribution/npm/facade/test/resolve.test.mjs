import assert from 'node:assert/strict';
import { chmodSync, cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import process from 'node:process';
import { after, test } from 'node:test';
import { fileURLToPath, pathToFileURL } from 'node:url';

const libDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'lib');
const SCOPE = '@svg-toolkit';
const TOOL = 'language-server';
const BIN = 'svg-language-server';

/** Every fixture root created by this file, torn down at exit. */
const roots = [];

after(() => {
	for (const root of roots) rmSync(root, { force: true, recursive: true });
});

/**
 * Build a throwaway install tree: a facade package carrying the real lib/
 * sources, plus the given platform packages under node_modules.
 *
 * @param {string[]} installed Target suffixes to materialise, e.g. `linux-x64-gnu`.
 * @param {string[]} [declared] Suffixes listed in optionalDependencies; defaults to `installed`.
 */
function makeFixture(installed, declared = installed) {
	const root = mkdtempSync(join(tmpdir(), 'svg-facade-'));
	roots.push(root);

	mkdirSync(join(root, 'lib'), { recursive: true });
	for (const file of ['resolve.mjs', 'launch.mjs', 'libc.mjs']) {
		cpSync(join(libDir, file), join(root, 'lib', file));
	}

	writeFileSync(
		join(root, 'package.json'),
		JSON.stringify({
			name: `${SCOPE}/${BIN}`,
			version: '0.0.0-test',
			type: 'module',
			imports: {
				'#launch': './lib/launch.mjs',
				'#libc': './lib/libc.mjs',
				'#pkg': './package.json',
				'#resolve': './lib/resolve.mjs',
			},
			repository: { url: 'git+https://github.com/kjanat/svg.git' },
			bugs: { url: 'https://github.com/kjanat/svg/issues' },
			optionalDependencies: Object.fromEntries(
				declared.map((suffix) => [`${SCOPE}/${TOOL}-${suffix}`, '0.0.0-test']),
			),
		}),
	);

	// ansispeck is a real dependency of the facade; a pass-through stand-in
	// keeps this test about resolution rather than about colour codes.
	const ansispeck = join(root, 'node_modules', 'ansispeck');
	mkdirSync(ansispeck, { recursive: true });
	writeFileSync(join(ansispeck, 'package.json'), JSON.stringify({ name: 'ansispeck', version: '0.0.0', type: 'module', main: 'index.mjs' }));
	writeFileSync(
		join(ansispeck, 'index.mjs'),
		['bold', 'cyan', 'red', 'yellow'].map((fn) => `export const ${fn} = (s) => String(s);\n`).join('')
			+ 'export const link = (text) => String(text);\n',
	);

	for (const suffix of installed) {
		const pkgDir = join(root, 'node_modules', SCOPE, `${TOOL}-${suffix}`);
		mkdirSync(join(pkgDir, 'bin'), { recursive: true });
		writeFileSync(join(pkgDir, 'package.json'), JSON.stringify({ name: `${SCOPE}/${TOOL}-${suffix}`, version: '0.0.0-test' }));
		writeFileSync(join(pkgDir, 'bin', BIN), '#!/bin/sh\nexit 0\n');
		chmodSync(join(pkgDir, 'bin', BIN), 0o755);
	}

	return root;
}

/**
 * Import a fixture's own copy of resolve.mjs and resolve the binary under the
 * given libc, capturing anything written to the console.
 *
 * @param {string} root
 * @param {'musl' | 'glibc' | undefined} libc
 */
async function resolveIn(root, libc) {
	const previous = process.env.SVG_LIBC;
	if (libc === undefined) delete process.env.SVG_LIBC;
	else process.env.SVG_LIBC = libc;

	const output = [];
	const realError = console.error;
	const realWarn = console.warn;
	console.error = (...args) => output.push(args.join(' '));
	console.warn = (...args) => output.push(args.join(' '));

	try {
		// Cache-bust so each fixture loads its own module instance.
		const { resolveBinary } = await import(`${pathToFileURL(join(root, 'lib', 'resolve.mjs')).href}?t=${root}`);
		return { path: resolveBinary(BIN), output: output.join('\n'), error: null };
	} catch (err) {
		return { path: null, output: output.join('\n'), error: err };
	} finally {
		console.error = realError;
		console.warn = realWarn;
		if (previous === undefined) delete process.env.SVG_LIBC;
		else process.env.SVG_LIBC = previous;
	}
}

test('musl host with both variants installed selects the musl package', async () => {
	const root = makeFixture(['linux-x64-gnu', 'linux-x64-musl']);
	const { path, error } = await resolveIn(root, 'musl');
	assert.equal(error, null);
	assert.ok(path?.includes(`${TOOL}-linux-x64-musl`), `expected the musl package, got ${path}`);
});

test('musl host with only the gnu sibling fails with a libc diagnostic, not ENOENT', async () => {
	const root = makeFixture(['linux-x64-gnu']);
	const { path, output, error } = await resolveIn(root, 'musl');

	assert.equal(path, null);
	assert.match(String(error?.message), /musl/i);
	assert.match(output, /musl/i, 'the diagnostic must name the libc mismatch');
	assert.match(output, /language-server-linux-x64-musl/, 'and name the package that was expected');
	assert.doesNotMatch(output, /ENOENT/);
});

test('glibc host is unchanged when both variants are installed', async () => {
	const root = makeFixture(['linux-x64-gnu', 'linux-x64-musl']);
	const { path, error } = await resolveIn(root, 'glibc');
	assert.equal(error, null);
	assert.ok(path?.includes(`${TOOL}-linux-x64-gnu`), `expected the gnu package, got ${path}`);
});

test('glibc host falls back to the static musl build when gnu is absent', async () => {
	const root = makeFixture(['linux-x64-musl']);
	const { path, error } = await resolveIn(root, 'glibc');
	assert.equal(error, null);
	assert.ok(path?.includes(`${TOOL}-linux-x64-musl`), `expected the musl fallback, got ${path}`);
});

test('musl host honours the armv7 pair suffixes', async () => {
	const root = makeFixture(['linux-armv7-gnueabihf', 'linux-armv7-musleabihf']);
	const { path, error } = await resolveIn(root, 'musl');
	assert.equal(error, null);
	assert.ok(path?.includes(`${TOOL}-linux-armv7-musleabihf`), `expected the musl armv7 package, got ${path}`);
});

// The reproduction the issue is really about: a tree carrying both variants,
// on a real host, with nothing overriding detection. CI runs this file twice —
// once in an Alpine container (EXPECT_LIBC=musl) and once on the stock glibc
// runner — so the musl path is exercised on an actual musl host.
test("a both-variants tree resolves to this host's libc with no override", { skip: hostLibcSkip() }, async () => {
	const expected = process.env.EXPECT_LIBC;
	const root = makeFixture(['linux-x64-gnu', 'linux-x64-musl']);
	const { path, output, error } = await resolveIn(root, undefined);

	assert.equal(error, null, output);
	assert.ok(
		path?.includes(`${TOOL}-linux-x64-${expected === 'musl' ? 'musl' : 'gnu'}`),
		`expected the ${expected} package, got ${path}`,
	);
	assert.doesNotMatch(output, /could not be identified/, 'the host libc must be identified without help');
});

test('a gnu-only tree on a musl host refuses instead of spawning', {
	skip: hostLibcSkip() || (process.env.EXPECT_LIBC !== 'musl' && 'musl hosts only'),
}, async () => {
	const root = makeFixture(['linux-x64-gnu']);
	const { path, output, error } = await resolveIn(root, undefined);

	assert.equal(path, null);
	assert.match(String(error?.message), /musl/i);
	assert.match(output, /language-server-linux-x64-musl/);
});

function hostLibcSkip() {
	const expected = process.env.EXPECT_LIBC;
	if (expected === undefined) return 'set EXPECT_LIBC=glibc|musl to run';
	if (expected !== 'glibc' && expected !== 'musl') return `EXPECT_LIBC must be glibc or musl, got ${expected}`;
	return false;
}

test('a declared but uninstalled package is skipped rather than fatal', async () => {
	const root = makeFixture(['linux-x64-musl'], ['linux-x64-gnu', 'linux-x64-musl']);
	const { path, error } = await resolveIn(root, 'musl');
	assert.equal(error, null);
	assert.ok(path?.includes(`${TOOL}-linux-x64-musl`));
});
