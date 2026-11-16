import assert from 'node:assert/strict';
import { join } from 'node:path';
import process from 'node:process';
import { after, test } from 'node:test';
import { pathToFileURL } from 'node:url';
import { BIN, FOREIGN, GNU, HOST, makeFixture, MUSL, removeFixtures, TOOL } from './fixture.mjs';

after(removeFixtures);

/**
 * Resolve through a fixture's own resolve.mjs, capturing console output.
 *
 * @param {string} root
 * @param {string | undefined} libc Value for SVG_LIBC, or `undefined` to leave it unset.
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

// SVG_LIBC only applies on Linux, so the override-driven tests below run on Linux hosts only.
const linuxOnly = { skip: process.platform !== 'linux' && 'Linux hosts only' };
const posixOnly = { skip: process.platform === 'win32' && 'POSIX hosts only' };

test('a non-Linux host prints no libc warning with both Linux variants installed', {
	skip: process.platform === 'linux' && 'non-Linux hosts only',
}, async () => {
	const root = makeFixture(['linux-x64-gnu', 'linux-x64-musl', HOST]);
	const { path, output, error } = await resolveIn(root, undefined);
	assert.equal(error, null, output);
	assert.ok(path?.includes(`${TOOL}-${HOST}`), `expected the ${HOST} package, got ${path}`);
	assert.doesNotMatch(output, /could not be identified/);
});

test("a package for another OS or CPU is never chosen over this host's", async () => {
	const root = makeFixture([FOREIGN, HOST]);
	const { path, output, error } = await resolveIn(root, process.platform === 'linux' ? 'glibc' : undefined);
	assert.equal(error, null, output);
	assert.ok(path?.includes(`${TOOL}-${HOST}`), `expected the ${HOST} package, got ${path}`);
});

test('a tree with only foreign builds reports an unsupported platform, not a skipped install', async () => {
	const root = makeFixture([FOREIGN]);
	const { path, output, error } = await resolveIn(root, undefined);
	assert.equal(path, null);
	assert.match(String(error?.message), /No prebuilt binary is published for/);
	assert.match(output, /no prebuilt binary is published for/);
	assert.match(output, new RegExp(`${TOOL}-${FOREIGN}`), 'the published builds are listed');
	assert.doesNotMatch(output, /optionalDependencies/);
});

test('a binary without its executable bit is reported, not spawned for EACCES', posixOnly, async () => {
	const root = makeFixture([HOST], { mode: 0o644 });
	const { path, output, error } = await resolveIn(root, process.platform === 'linux' ? 'glibc' : undefined);
	assert.equal(path, null);
	assert.ok(error);
	assert.match(output, /is not executable; reinstall the package or run chmod \+x on it/);
});

test('a facade whose manifest has a shorthand repository and no bugs still resolves', async () => {
	const root = makeFixture([HOST], { repository: 'kjanat/svg', bugs: undefined });
	const { path, output, error } = await resolveIn(root, process.platform === 'linux' ? 'glibc' : undefined);
	assert.equal(error, null, output);
	assert.ok(path?.includes(`${TOOL}-${HOST}`));
	const missing = await resolveIn(makeFixture([FOREIGN], { repository: 'kjanat/svg', bugs: undefined }), undefined);
	assert.match(missing.output, /cargo install --git=https:\/\/github\.com\/kjanat\/svg\/ svg-language-server/);
	assert.match(missing.output, /https:\/\/github\.com\/kjanat\/svg\/issues/);
});

test('an SVG_LIBC value that names no libc is reported and ignored', async () => {
	const root = makeFixture([HOST]);
	const { path, output, error } = await resolveIn(root, 'uclibc');
	assert.match(output, /ignoring SVG_LIBC=uclibc; use musl, glibc or gnu\./);
	if (process.platform !== 'linux') {
		assert.equal(error, null, output);
		assert.ok(path?.includes(`${TOOL}-${HOST}`));
	}
});

test('musl host with both variants installed selects the musl package', linuxOnly, async () => {
	const root = makeFixture([GNU, MUSL]);
	const { path, error } = await resolveIn(root, 'musl');
	assert.equal(error, null);
	assert.ok(path?.includes(`${TOOL}-${MUSL}`), `expected the musl package, got ${path}`);
});

test('musl host with only the gnu sibling fails with a libc diagnostic, not ENOENT', linuxOnly, async () => {
	const root = makeFixture([GNU], { declared: [GNU, MUSL, 'linux-riscv64-gnu'] });
	const { path, output, error } = await resolveIn(root, 'musl');

	assert.equal(path, null);
	assert.match(String(error?.message), /musl/i);
	assert.match(output, /musl/i, 'the diagnostic must name the libc mismatch');
	assert.match(output, new RegExp(`Installed but unusable:\\n  - @svg-toolkit/${TOOL}-${GNU}\\n`));
	assert.match(output, new RegExp(`Expected instead:\\n  - @svg-toolkit/${TOOL}-${MUSL}\\n`));
	assert.doesNotMatch(output, /ENOENT/);
});

test('musl host with nothing installed gets the install diagnostic, not a libc mismatch', linuxOnly, async () => {
	const root = makeFixture([], { declared: [GNU, MUSL] });
	const { path, output, error } = await resolveIn(root, 'musl');
	assert.equal(path, null);
	assert.match(String(error?.message), /No prebuilt binary found/);
	assert.match(output, /skipped optionalDependencies/);
	assert.doesNotMatch(output, /only glibc builds are installed/);
});

test('glibc host is unchanged when both variants are installed', linuxOnly, async () => {
	const root = makeFixture([GNU, MUSL]);
	const { path, error } = await resolveIn(root, 'glibc');
	assert.equal(error, null);
	assert.ok(path?.includes(`${TOOL}-${GNU}`), `expected the gnu package, got ${path}`);
});

test('SVG_LIBC=gnu chooses the glibc package', linuxOnly, async () => {
	const root = makeFixture([GNU, MUSL], { declared: [MUSL, GNU] });
	const { path, error } = await resolveIn(root, 'gnu');
	assert.equal(error, null);
	assert.ok(path?.includes(`${TOOL}-${GNU}`), `expected the gnu package, got ${path}`);
});

test('glibc host falls back to the static musl build when gnu is absent', linuxOnly, async () => {
	const root = makeFixture([MUSL]);
	const { path, error } = await resolveIn(root, 'glibc');
	assert.equal(error, null);
	assert.ok(path?.includes(`${TOOL}-${MUSL}`), `expected the musl fallback, got ${path}`);
});

// The reproduction: both variants installed, real host, nothing overriding detection.
test("a both-variants tree resolves to this host's libc with no override", { skip: hostLibcSkip() }, async () => {
	const expected = process.env.EXPECT_LIBC;
	const root = makeFixture([GNU, MUSL]);
	const { path, output, error } = await resolveIn(root, undefined);

	assert.equal(error, null, output);
	assert.ok(path?.includes(`${TOOL}-${expected === 'musl' ? MUSL : GNU}`), `expected the ${expected} package, got ${path}`);
	assert.doesNotMatch(output, /could not be identified/, 'the host libc must be identified without help');
});

test('a gnu-only tree on a musl host refuses instead of spawning', {
	skip: hostLibcSkip() || (process.env.EXPECT_LIBC !== 'musl' && 'musl hosts only'),
}, async () => {
	const root = makeFixture([GNU]);
	const { path, output, error } = await resolveIn(root, undefined);

	assert.equal(path, null);
	assert.match(String(error?.message), /musl/i);
	assert.match(output, new RegExp(`${TOOL}-${MUSL}`));
});

function hostLibcSkip() {
	const expected = process.env.EXPECT_LIBC;
	if (expected === undefined) return 'set EXPECT_LIBC=glibc|musl to run';
	if (expected !== 'glibc' && expected !== 'musl') return `EXPECT_LIBC must be glibc or musl, got ${expected}`;
	return false;
}

test('a declared but uninstalled package is skipped rather than fatal', linuxOnly, async () => {
	const root = makeFixture([MUSL], { declared: [GNU, MUSL] });
	const { path, error } = await resolveIn(root, 'musl');
	assert.equal(error, null);
	assert.ok(path?.includes(`${TOOL}-${MUSL}`));
});
