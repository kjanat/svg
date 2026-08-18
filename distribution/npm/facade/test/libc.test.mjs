import { classifyLibc, detectLibc, LIBC_ENV, planCandidates, siblingName } from '#libc';
import assert from 'node:assert/strict';
import process from 'node:process';
import { test } from 'node:test';

const SCOPE = '@svg-toolkit/language-server';

/** The linux half of the published matrix, in the order build-packages.ts emits. */
const LINUX_X64 = [`${SCOPE}-linux-x64-gnu`, `${SCOPE}-linux-x64-musl`];
const ARMV7 = [`${SCOPE}-linux-armv7-gnueabihf`, `${SCOPE}-linux-armv7-musleabihf`];
const UNPAIRED = [`${SCOPE}-linux-riscv64-gnu`, `${SCOPE}-linux-s390x-gnu`];
const NON_LINUX = [`${SCOPE}-darwin-arm64`, `${SCOPE}-win32-x64-msvc`];

const noFiles = () => false;
const noDirs = () => [];
const noReport = () => undefined;

/** detectLibc with every host signal silenced, so a test only enables what it asserts on. */
const detect = (options) =>
	detectLibc({
		platform: 'linux',
		arch: 'x64',
		env: {},
		report: noReport,
		fileExists: noFiles,
		dirEntries: noDirs,
		...options,
	});

test('classifyLibc reads the pair suffix, not a literal gnu/musl', () => {
	assert.equal(classifyLibc(`${SCOPE}-linux-x64-gnu`)?.libc, 'glibc');
	assert.equal(classifyLibc(`${SCOPE}-linux-x64-musl`)?.libc, 'musl');
	// armv7 pairs as gnueabihf/musleabihf; neither may be read as gnu/musl.
	assert.equal(classifyLibc(`${SCOPE}-linux-armv7-gnueabihf`)?.libc, 'glibc');
	assert.equal(classifyLibc(`${SCOPE}-linux-armv7-musleabihf`)?.libc, 'musl');
	assert.equal(classifyLibc(`${SCOPE}-darwin-arm64`), null);
	assert.equal(classifyLibc(`${SCOPE}-win32-x64-msvc`), null);
});

test('siblingName round-trips both pair shapes', () => {
	assert.equal(siblingName(`${SCOPE}-linux-x64-gnu`), `${SCOPE}-linux-x64-musl`);
	assert.equal(siblingName(`${SCOPE}-linux-x64-musl`), `${SCOPE}-linux-x64-gnu`);
	assert.equal(siblingName(`${SCOPE}-linux-armv7-gnueabihf`), `${SCOPE}-linux-armv7-musleabihf`);
	assert.equal(siblingName(`${SCOPE}-linux-armv7-musleabihf`), `${SCOPE}-linux-armv7-gnueabihf`);
	assert.equal(siblingName(`${SCOPE}-darwin-arm64`), null);
});

test('musl host selects the musl package even though gnu is declared first', () => {
	const { candidates, dropped } = planCandidates(LINUX_X64, 'musl');
	assert.equal(candidates[0], `${SCOPE}-linux-x64-musl`);
	assert.deepEqual(dropped, [`${SCOPE}-linux-x64-gnu`]);
	assert.ok(!candidates.includes(`${SCOPE}-linux-x64-gnu`));
});

test('musl host with only the gnu sibling installed has nothing to run', () => {
	const { candidates, dropped } = planCandidates([`${SCOPE}-linux-x64-gnu`], 'musl');
	assert.deepEqual(candidates, []);
	assert.deepEqual(dropped, [`${SCOPE}-linux-x64-gnu`]);
});

test('musl host handles the armv7 pair by its own suffixes', () => {
	const { candidates, dropped } = planCandidates(ARMV7, 'musl');
	assert.deepEqual(candidates, [`${SCOPE}-linux-armv7-musleabihf`]);
	assert.deepEqual(dropped, [`${SCOPE}-linux-armv7-gnueabihf`]);
});

test('glibc host keeps gnu first and the static musl build as a fallback', () => {
	const { candidates, dropped, undecided } = planCandidates(LINUX_X64, 'glibc');
	assert.deepEqual(candidates, [`${SCOPE}-linux-x64-gnu`, `${SCOPE}-linux-x64-musl`]);
	assert.deepEqual(dropped, []);
	assert.equal(undecided, false);
});

test('glibc host still falls back to musl when gnu is absent', () => {
	const { candidates } = planCandidates([`${SCOPE}-linux-x64-musl`], 'glibc');
	assert.deepEqual(candidates, [`${SCOPE}-linux-x64-musl`]);
});

test('unpaired gnu-only targets keep the declared order', () => {
	assert.deepEqual(planCandidates(UNPAIRED, 'glibc').candidates, UNPAIRED);
	assert.deepEqual(planCandidates(UNPAIRED, null).candidates, UNPAIRED);
	assert.equal(planCandidates(UNPAIRED, null).undecided, false);
});

test('non-linux packages keep the declared order and are never dropped', () => {
	for (const libc of ['glibc', 'musl', null]) {
		const { candidates, dropped } = planCandidates(NON_LINUX, libc);
		assert.deepEqual(candidates, NON_LINUX);
		assert.deepEqual(dropped, []);
	}
});

test('undecided libc with both variants declared is flagged, not silently guessed', () => {
	const { candidates, undecided } = planCandidates(LINUX_X64, null);
	assert.equal(undecided, true);
	assert.deepEqual(candidates, LINUX_X64, 'declared order is preserved so behaviour is unchanged');
});

test(`${LIBC_ENV} overrides every other signal`, () => {
	assert.equal(detect({ env: { [LIBC_ENV]: 'musl' } }), 'musl');
	assert.equal(detect({ env: { [LIBC_ENV]: ' GLIBC ' } }), 'glibc');
	// A nonsense value must not be treated as an answer.
	assert.equal(detect({ env: { [LIBC_ENV]: 'uclibc' } }), null);
});

test('glibcVersionRuntime settles the question without touching the filesystem', () => {
	assert.equal(detect({ report: () => ({ header: { glibcVersionRuntime: '2.39' } }) }), 'glibc');
	// Present but empty is not proof.
	assert.equal(detect({ report: () => ({ header: { glibcVersionRuntime: '' } }) }), null);
});

test('a throwing process.report does not break detection', () => {
	const detected = detect({
		report: () => {
			throw new Error('report unavailable');
		},
		fileExists: (path) => path === '/etc/alpine-release',
	});
	assert.equal(detected, 'musl');
});

test('musl is detected from alpine, the arch loader, or a /lib scan', () => {
	assert.equal(detect({ fileExists: (path) => path === '/etc/alpine-release' }), 'musl');
	assert.equal(detect({ fileExists: (path) => path === '/lib/ld-musl-x86_64.so.1' }), 'musl');
	assert.equal(detect({ dirEntries: (dir) => (dir === '/lib' ? ['ld-musl-x86_64.so.1'] : []) }), 'musl');
	// The arch loader name must follow process.arch, not x64.
	assert.equal(
		detect({ arch: 'arm64', fileExists: (path) => path === '/lib/ld-musl-aarch64.so.1' }),
		'musl',
	);
});

test('glibc is detected from its ELF interpreter or libc.so.6', () => {
	assert.equal(detect({ fileExists: (path) => path === '/lib64/ld-linux-x86-64.so.2' }), 'glibc');
	assert.equal(detect({ fileExists: (path) => path === '/lib/libc.so.6' }), 'glibc');
	assert.equal(
		detect({ arch: 'arm64', fileExists: (path) => path === '/lib/ld-linux-aarch64.so.1' }),
		'glibc',
	);
});

test('musl wins over glibc markers, because a musl host may carry a glibc shim', () => {
	const detected = detect({
		fileExists: (path) => path === '/etc/alpine-release' || path === '/lib/libc.so.6',
	});
	assert.equal(detected, 'musl');
});

test('non-linux platforms are never libc-classified', () => {
	for (const platform of ['darwin', 'win32', 'freebsd']) {
		assert.equal(
			detectLibc({ platform, arch: 'x64', env: { [LIBC_ENV]: 'musl' }, report: noReport, fileExists: () => true, dirEntries: noDirs }),
			null,
		);
	}
});

test('an unidentifiable host reports null rather than guessing', () => {
	assert.equal(detect({}), null);
});

// No injection: checks the marker lists against a real filesystem. CI sets EXPECT_LIBC per job.
test('detects the libc of the host it is running on', { skip: hostLibcSkip() }, () => {
	const env = { ...process.env };
	// The override would make this assert on itself.
	delete env[LIBC_ENV];
	assert.equal(detectLibc({ env }), process.env.EXPECT_LIBC);
});

function hostLibcSkip() {
	const expected = process.env.EXPECT_LIBC;
	if (expected === undefined) return 'set EXPECT_LIBC=glibc|musl to run';
	if (expected !== 'glibc' && expected !== 'musl') return `EXPECT_LIBC must be glibc or musl, got ${expected}`;
	return false;
}
