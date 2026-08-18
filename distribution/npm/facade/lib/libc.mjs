import { existsSync, readdirSync } from 'node:fs';
import process from 'node:process';

/**
 * Paired libc suffixes on `<scope>/<tool>-<target>` platform packages, longest
 * first so `gnueabihf` is never read as `gnu`. The pair is looked up from the
 * package name rather than synthesised from a literal, because armv7 pairs as
 * `gnueabihf`/`musleabihf` while everything else pairs as `gnu`/`musl`.
 */
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
	ppc64: 'ld-musl-powerpc64.so.1',
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

/** Environment variable that overrides libc detection. */
export const LIBC_ENV = 'SVG_LIBC';

/**
 * `existsSync` that cannot throw. A locked-down root (or a path that is not
 * traversable) raises rather than returning false, and a probe must never be
 * the thing that breaks the launcher.
 *
 * @param {string} path
 * @returns {boolean}
 */
export function exists(path) {
	try {
		return existsSync(path);
	} catch {
		return false;
	}
}

/**
 * `readdirSync` that cannot throw, yielding an empty listing for any directory
 * that is missing or unreadable.
 *
 * @param {string} dir
 * @returns {string[]}
 */
export function listDir(dir) {
	try {
		return readdirSync(dir);
	} catch {
		return [];
	}
}

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
 * Identify the host libc from layered signals, most authoritative first. Each
 * layer is positive proof of one libc rather than an inference from another
 * layer's absence, and musl is probed before glibc because a musl host may
 * carry a glibc compatibility shim and match both.
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

	const override = env[LIBC_ENV]?.trim().toLowerCase();
	if (override === 'musl' || override === 'glibc') return override;

	// Node populates glibcVersionRuntime only when it linked against glibc,
	// which settles the question without touching the filesystem.
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
 * Order the declared platform packages so the one matching the host libc is
 * tried first, and remove the ones that cannot run at all.
 *
 * A glibc build on a musl host is not a fallback — its ELF interpreter is
 * absent, so it is dropped rather than spawned for an unexplained `ENOENT`.
 * The reverse is not true: the musl builds are static, so on a glibc host they
 * stay as a fallback behind the glibc build.
 *
 * @param {string[]} subPackages Declared optional dependencies, in manifest order.
 * @param {'glibc' | 'musl' | null} libc
 * @returns {{ candidates: string[], dropped: string[], undecided: boolean }}
 *   `dropped` lists packages excluded as unrunnable. `undecided` is set when
 *   both variants of some target are declared but no signal identified the
 *   host, meaning manifest order alone would decide.
 */
export function planCandidates(subPackages, libc) {
	if (libc === null) {
		const declared = new Set(subPackages);
		const undecided = subPackages.some((name) => {
			const sibling = siblingName(name);
			return sibling !== null && declared.has(sibling);
		});
		return { candidates: [...subPackages], dropped: [], undecided };
	}

	/** @type {string[]} */ const preferred = [];
	/** @type {string[]} */ const fallback = [];
	/** @type {string[]} */ const dropped = [];

	for (const name of subPackages) {
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

	return { candidates: [...preferred, ...fallback], dropped, undecided: false };
}
