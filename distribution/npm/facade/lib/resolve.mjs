import { detectLibc, exists, LIBC_ENV, libcOverride, planCandidates, siblingName } from '#libc';
import { bold, cyan, link, red, yellow } from 'ansispeck';
import { accessSync, constants } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { arch, platform } from 'node:process';

const require = createRequire(import.meta.url);
// repository.url (canonical `git+<url>.git` form) and bugs.url are stamped
// into every published facade by build-packages.ts from Cargo.toml's
// `repository`; the strip below undoes exactly that stamped decoration.
const { optionalDependencies, name: pkgName, repository, bugs } = require('#pkg');

/**
 * Browsable repository URL from a manifest `repository`, which npm also
 * accepts as a plain `owner/repo` or `github:owner/repo` string.
 *
 * @param {unknown} value
 * @returns {string | undefined}
 */
function repositoryHome(value) {
	if (typeof value !== 'string' || value === '') return undefined;
	const url = value.replace(/^git\+/, '').replace(/\.git$/, '').replace(/^github:/, '');
	return /^[a-z][a-z+.-]*:/i.test(url) ? url : `https://github.com/${url}`;
}

const repo = repositoryHome(typeof repository === 'string' ? repository : repository?.url);
const reported = typeof bugs === 'string' ? bugs : bugs?.url;
const issues = typeof reported === 'string' ? reported : repo === undefined ? undefined : `${repo}/issues`;
const subPackages = Object.keys(optionalDependencies || {});
const indent = '  ';

/** @type {(name: string) => boolean} */
const installed = (name) => {
	try {
		require.resolve(`${name}/package.json`);
		return true;
	} catch {
		return false;
	}
};

/** @type {(path: string) => boolean} */
const executable = (path) => {
	if (platform === 'win32') return true;
	try {
		accessSync(path, constants.X_OK);
		return true;
	} catch {
		return false;
	}
};

/** @param {string} reason */
const issueLine = (reason) => issues === undefined ? '' : `\n${indent}- ${reason}: ${link(issues, issues)}`;

/** @param {string} name */
const sourceLine = (name) => repo === undefined ? '' : `\n${indent}- install from source: ${cyan(`cargo install --git=${repo}/ ${name}`)}`;

/**
 * Report that only glibc builds are installed on a musl host, and refuse.
 *
 * @param {string[]} unusable Installed platform packages that need glibc.
 * @param {string[]} wanted Declared musl siblings of those packages.
 * @returns {never}
 */
function failMuslMismatch(unusable, wanted) {
	const install = wanted.length > 0
		? `\n${indent}- install the musl package explicitly: ${cyan(`npm install ${wanted.join(' ')}`)}`
		: '';
	console.error(
		`${red(pkgName)}: this host uses ${bold('musl')} libc, but only ${
			yellow('glibc')
		} builds are installed. A glibc binary cannot run here — its ELF interpreter does not exist.

Installed but unusable:
${indent}- ${unusable.join(`\n${indent}- `)}
${wanted.length > 0 ? `\nExpected instead:\n${indent}- ${wanted.join(`\n${indent}- `)}\n` : ''}
${bold('bun')} and ${bold('deno')} install every optional dependency regardless of the ${cyan('libc')} field,
so both variants land in ${cyan('node_modules')}; copying a ${cyan('node_modules')} between hosts does the same.

Workarounds:
${indent}- reinstall with ${cyan('npm')}, ${cyan('pnpm')} or ${cyan('yarn')}, which honour ${cyan('libc')}${install}
${indent}- override detection if it is wrong: ${cyan(`${LIBC_ENV}=glibc`)}${issueLine('file an issue if your platform should be supported')}
`,
	);

	throw new Error('Installed platform packages target glibc, but this host uses musl libc.');
}

/**
 * Report that no published build targets this host, and refuse.
 *
 * @param {string | null} stem The host target, or `null` for an arm host other than ARMv7 hard-float.
 * @param {string} name
 * @returns {never}
 */
function failUnsupported(stem, name) {
	const host = stem === null ? `${platform}-${arch} (only ARMv7 hard-float arm builds are published)` : stem;
	console.error(
		`${red(pkgName)}: no prebuilt binary is published for ${yellow(host)}.

Published builds:
${indent}- ${subPackages.join(`\n${indent}- `)}

Workarounds:${sourceLine(name)}${issueLine('file an issue to request this platform')}
`,
	);

	throw new Error(`No prebuilt binary is published for ${platform}-${arch}.`);
}

/**
 * Locate the prebuilt executable matching the current platform and architecture.
 *
 * Searches optional-dependency sub-packages for a matching `bin/<exe>` and returns its filesystem path.
 * If no candidate is found, an explanatory error message is written to stderr and an `Error` is thrown.
 *
 * @param {string} name - Base name of the executable (without platform-specific extension).
 * @returns {string} The filesystem path to the resolved executable.
 * @throws {Error} If no suitable binary is found for the current platform and architecture.
 */
export function resolveBinary(name) {
	const exe = platform === 'win32' ? `${name}.exe` : name;
	const errors = [];

	const override = libcOverride();
	if (override.kind === 'invalid') {
		console.warn(
			`${yellow(pkgName)}: ignoring ${cyan(`${LIBC_ENV}=${override.value}`)}; use ${cyan('musl')}, ${cyan('glibc')} or ${cyan('gnu')}.`,
		);
	}

	const libc = detectLibc();
	const { stem, candidates, dropped, undecided } = planCandidates(subPackages, libc, { installed });

	if (candidates.length === 0 && dropped.length === 0) failUnsupported(stem, name);

	if (undecided) {
		console.warn(
			`${yellow(pkgName)}: both glibc and musl builds are installed but the host libc could not be identified; falling back to manifest order. Set ${
				cyan(`${LIBC_ENV}=musl`)
			} or ${cyan(`${LIBC_ENV}=glibc`)} to choose explicitly.`,
		);
	}

	for (const subPkg of candidates) {
		let pkgJsonPath;
		try {
			pkgJsonPath = require.resolve(`${subPkg}/package.json`);
		} catch (err) {
			errors.push(`${subPkg}: ${err instanceof Error ? err.message : String(err)}`);
			continue;
		}
		const binPath = join(dirname(pkgJsonPath), 'bin', exe);
		// `require.resolve` proves the package.json exists, not the binary.
		// Could mismatch if a user manually deletes the bin, or a partial
		// install half-succeeded. Prefer a clear error here over an opaque
		// `ENOENT` from `spawnSync` later in `launch.mjs`.
		if (!exists(binPath)) {
			errors.push(`${subPkg}: package present but bin missing at ${binPath}`);
			continue;
		}
		if (!executable(binPath)) {
			errors.push(`${subPkg}: ${binPath} is not executable; reinstall the package or run chmod +x on it`);
			continue;
		}
		return binPath;
	}

	// Every installed build for this host targets glibc, so the install is not
	// merely incomplete — it is the wrong libc. Say that, instead of letting
	// the spawn report a bare `ENOENT` for a missing ELF interpreter.
	const unusable = libc === 'musl' ? dropped.filter(installed) : [];
	if (unusable.length > 0) {
		const wanted = unusable
			.map((pkg) => siblingName(pkg))
			.filter((pkg) => pkg !== null && subPackages.includes(pkg));
		failMuslMismatch(unusable, wanted);
	}

	const detail = errors.length > 0
		? '\n\nDetails of attempted resolutions:\n  - ' + errors.join('\n  - ')
		: '';

	const errorText = `${red(pkgName)}: no prebuilt binary found for ${yellow(stem ?? `${platform}-${arch}`)}.

This usually means your package manager skipped ${cyan('optionalDependencies')}
(common with ${cyan('--no-optional')}, ${cyan('--omit=optional')}, or some Docker/CI setups).

Workarounds:
${indent}- reinstall without: ${cyan('--no-optional')} / ${cyan('--omit=optional')}
${indent}- bun + ${cyan('minimumReleaseAge')}: add the platform packages (not just ${bold(pkgName)}) to ${
	cyan('minimumReleaseAgeExcludes')
} — a fresh release is otherwise age-gated${sourceLine(name)}${issueLine('file an issue if your platform is unsupported')}${detail}
`;

	console.error(errorText);

	throw new Error('No prebuilt binary found for the current platform and architecture.');
}
