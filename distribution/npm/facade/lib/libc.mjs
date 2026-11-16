import { existsSync, readdirSync } from 'node:fs';
import process from 'node:process';

/** Paired libc suffixes, longest first so `gnueabihf` is not read as `gnu`. */
const LIBC_SUFFIX_PAIRS = [
	{ glibc: 'gnueabihf', musl: 'musleabihf' },
	{ glibc: 'gnu', musl: 'musl' },
];

/** musl dynamic loader per Node `process.arch`, as installed by musl-libc. */
const MUSL_LOADERS = {
	x64: 'ld-musl-x86_64.so.1',
	arm64: 'ld-musl-aarch64.so.1',
	arm: 'ld-musl-armhf.so.1',
	ia32: 'ld-musl-i386.so.1',
	riscv64: 'ld-musl-riscv64.so.1',
	ppc64: 'ld-musl-powerpc64le.so.1',
	s390x: 'ld-musl-s390x.so.1',
	loong64: 'ld-musl-loongarch64.so.1',
};

/** glibc ELF interpreter per Node `process.arch`. */
const GLIBC_LOADERS = {
	x64: ['/lib64/ld-linux-x86-64.so.2'],
	arm64: ['/lib/ld-linux-aarch64.so.1'],
	arm: ['/lib/ld-linux-armhf.so.3', '/lib/ld-linux.so.3'],
	ia32: ['/lib/ld-linux.so.2'],
	riscv64: ['/lib/ld-linux-riscv64-lp64d.so.1'],
	ppc64: ['/lib64/ld64.so.2'],
	s390x: ['/lib/ld64.so.1'],
	loong64: ['/lib64/ld-linux-loongarch-lp64d.so.1'],
};

const LOADER_DIRS = ['/lib', '/usr/lib', '/lib64'];

/** Package name spelling of a Node `process.arch` where the two differ. */
const PACKAGE_ARCH = { arm: 'armv7' };

/** Environment variable that overrides libc detection. */
export const LIBC_ENV = 'SVG_LIBC';

/** @type {Record<string, 'glibc' | 'musl'>} `gnu` is how the package names spell glibc. */
const LIBC_NAMES = { glibc: 'glibc', gnu: 'glibc', musl: 'musl' };

/**
 * Read the libc override.
 *
 * @param {Record<string, string | undefined>} [env]
 * @returns {{ kind: 'unset' } | { kind: 'set', libc: 'glibc' | 'musl' } | { kind: 'invalid', value: string }}
 */
export function libcOverride(env = process.env) {
	const value = env[LIBC_ENV]?.trim();
	if (!value) return { kind: 'unset' };
	const libc = LIBC_NAMES[value.toLowerCase()];
	return libc ? { kind: 'set', libc } : { kind: 'invalid', value };
}

/**
 * The `<os>-<cpu>` stem of the platform packages this host can run, such as
 * `linux-x64` or `darwin-arm64`. The arm packages are ARMv7 hard-float builds,
 * so an arm host whose Node reports an older ARM version or another float ABI
 * gets `null`. A Node build that reports neither is taken as ARMv7 hard-float.
 *
 * @param {object} [host] Injection points; the defaults read the real host.
 * @param {string} [host.platform]
 * @param {string} [host.arch]
 * @param {string} [host.armVersion] Node's `arm_version` build variable.
 * @param {string} [host.armFloatAbi] Node's `arm_float_abi` build variable.
 * @returns {string | null}
 */
export function hostTarget(host = {}) {
	const { platform = process.platform, arch = process.arch } = host;
	if (arch === 'arm') {
		const variables = process.config?.variables ?? {};
		const {
			armVersion = String(variables.arm_version ?? ''),
			armFloatAbi = String(variables.arm_float_abi ?? ''),
		} = host;
		if (armVersion !== '' && Number(armVersion) < 7) return null;
		if (armFloatAbi !== '' && armFloatAbi !== 'hard') return null;
	}
	return `${platform}-${PACKAGE_ARCH[arch] ?? arch}`;
}

/**
 * @param {string} name
 * @param {string} stem
 */
const buildsFor = (name, stem) => name.endsWith(`-${stem}`) || name.includes(`-${stem}-`);

/** A probe must never be what breaks the launcher: an unreadable root raises rather than returning false. */
const probe = (fallback, read) => {
	try {
		return read();
	} catch {
		return fallback;
	}
};

/** @type {(path: string) => boolean} */
export const exists = (path) => probe(false, () => existsSync(path));

/** @type {(dir: string) => string[]} */
export const listDir = (dir) => probe([], () => readdirSync(dir));

/**
 * Split a platform package name into its libc half.
 *
 * @param {string} name
 * @returns {{ libc: 'glibc' | 'musl', suffix: string, base: string, pair: { glibc: string, musl: string } } | null}
 *   `null` when the name carries no libc suffix (darwin, win32, freebsd, …).
 */
export function classifyLibc(name) {
	for (const pair of LIBC_SUFFIX_PAIRS) {
		for (const libc of /** @type {const} */ (['glibc', 'musl'])) {
			const suffix = pair[libc];
			if (name.endsWith(`-${suffix}`)) {
				return { libc, suffix, base: name.slice(0, -(suffix.length + 1)), pair };
			}
		}
	}
	return null;
}

/**
 * Name of the same platform package built against the other libc.
 *
 * @param {string} name
 * @returns {string | null} `null` for names with no libc suffix.
 */
export function siblingName(name) {
	const classified = classifyLibc(name);
	if (classified === null) return null;
	const other = classified.libc === 'musl' ? classified.pair.glibc : classified.pair.musl;
	return `${classified.base}-${other}`;
}

/**
 * @param {string} arch
 * @param {(path: string) => boolean} fileExists
 * @param {(dir: string) => string[]} dirEntries
 */
function hasMuslMarkers(arch, fileExists, dirEntries) {
	if (fileExists('/etc/alpine-release')) return true;

	const loader = MUSL_LOADERS[arch];
	if (loader !== undefined && LOADER_DIRS.some((dir) => fileExists(`${dir}/${loader}`))) return true;

	return LOADER_DIRS.some((dir) => dirEntries(dir).some((entry) => entry.startsWith('ld-musl-')));
}

/**
 * @param {string} arch
 * @param {(path: string) => boolean} fileExists
 * @param {(dir: string) => string[]} dirEntries
 */
function hasGlibcMarkers(arch, fileExists, dirEntries) {
	const loaders = GLIBC_LOADERS[arch] ?? [];
	if (loaders.some((path) => fileExists(path))) return true;
	if (LOADER_DIRS.some((dir) => fileExists(`${dir}/libc.so.6`))) return true;

	return LOADER_DIRS.some((dir) => dirEntries(dir).some((entry) => entry.startsWith('ld-linux') || entry === 'libc.so.6'));
}

/**
 * Identify the host libc, most authoritative signal first. musl is probed
 * before glibc: a musl host with a glibc shim matches both.
 *
 * @param {object} [options] Injection points; the defaults read the real host.
 * @param {string} [options.platform]
 * @param {string} [options.arch]
 * @param {Record<string, string | undefined>} [options.env]
 * @param {() => { header?: { glibcVersionRuntime?: unknown } } | undefined} [options.report]
 * @param {(path: string) => boolean} [options.fileExists]
 * @param {(dir: string) => string[]} [options.dirEntries]
 * @returns {'glibc' | 'musl' | null} `null` when nothing identified the host.
 */
export function detectLibc(options = {}) {
	const {
		platform = process.platform,
		arch = process.arch,
		env = process.env,
		report = () => process.report?.getReport?.(),
		fileExists = exists,
		dirEntries = listDir,
	} = options;

	if (platform !== 'linux') return null;

	const override = libcOverride(env);
	if (override.kind === 'set') return override.libc;

	// Only set when Node itself linked against glibc.
	let header;
	try {
		header = report()?.header;
	} catch {
		header = undefined;
	}
	if (typeof header?.glibcVersionRuntime === 'string' && header.glibcVersionRuntime !== '') return 'glibc';

	if (hasMuslMarkers(arch, fileExists, dirEntries)) return 'musl';
	if (hasGlibcMarkers(arch, fileExists, dirEntries)) return 'glibc';
	return null;
}

/**
 * Keep only the builds for this host's OS and CPU, order the matching libc
 * first and drop what cannot run. A glibc build on a musl host has no ELF
 * interpreter, so it is dropped rather than spawned for an unexplained
 * `ENOENT`; musl builds are static, so on glibc they stay a fallback.
 *
 * @param {string[]} subPackages Declared optional dependencies, in manifest order.
 * @param {'glibc' | 'musl' | null} libc
 * @param {object} [host] Injection points; the defaults read the real host.
 * @param {string} [host.platform]
 * @param {string} [host.arch]
 * @param {string} [host.armVersion]
 * @param {string} [host.armFloatAbi]
 * @param {(name: string) => boolean} [host.installed]
 * @returns {{ stem: string | null, candidates: string[], dropped: string[], undecided: boolean }}
 *   `stem` is the host's target from {@link hostTarget}. `undecided` means a
 *   Linux host of unidentified libc has both variants of its own target
 *   installed, so manifest order alone would decide.
 */
export function planCandidates(subPackages, libc, host = {}) {
	const { platform = process.platform, installed = () => true } = host;
	const stem = hostTarget(host);
	const own = stem === null ? [] : subPackages.filter((name) => buildsFor(name, stem));

	if (libc === null) {
		const present = new Set(own.filter(installed));
		const undecided = platform === 'linux' && [...present].some((name) => {
			const sibling = siblingName(name);
			return sibling !== null && present.has(sibling);
		});
		return { stem, candidates: own, dropped: [], undecided };
	}

	/** @type {string[]} */ const preferred = [];
	/** @type {string[]} */ const fallback = [];
	/** @type {string[]} */ const dropped = [];

	for (const name of own) {
		const classified = classifyLibc(name);
		if (classified === null) {
			fallback.push(name);
		} else if (classified.libc === libc) {
			preferred.push(name);
		} else if (libc === 'musl') {
			dropped.push(name);
		} else {
			fallback.push(name);
		}
	}

	return { stem, candidates: [...preferred, ...fallback], dropped, undecided: false };
}
