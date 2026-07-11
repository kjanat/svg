import { bold, cyan, link, red, yellow } from 'ansispeck';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { arch, platform } from 'node:process';

const require = createRequire(import.meta.url);
// repository.url (canonical `git+<url>.git` form) and bugs.url are stamped
// into every published facade by build-packages.ts from Cargo.toml's
// `repository`; the strip below undoes exactly that stamped decoration.
const { optionalDependencies, name: pkgName, repository, bugs } = require('#pkg');

const repo = repository.url.replace(/^git\+/, '').replace(/\.git$/, '');
const issues = bugs.url;
const subPackages = Object.keys(optionalDependencies || {});

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
	for (const subPkg of subPackages) {
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
		if (!existsSync(binPath)) {
			errors.push(`${subPkg}: package present but bin missing at ${binPath}`);
			continue;
		}
		return binPath;
	}

	const detail = errors.length > 0
		? '\n\nDetails of attempted resolutions:\n  - ' + errors.join('\n  - ')
		: '';

	const indent = '  ';

	const errorText = `${red(pkgName)}: no prebuilt binary found for ${yellow(`${platform}-${arch}`)}.

This usually means your package manager skipped ${cyan('optionalDependencies')}
(common with ${cyan('--no-optional')}, ${cyan('--omit=optional')}, or some Docker/CI setups).

Workarounds:
${indent}- reinstall without: ${cyan('--no-optional')} / ${cyan('--omit=optional')}
${indent}- bun + ${cyan('minimumReleaseAge')}: add the platform packages (not just ${bold(pkgName)}) to ${
		cyan('minimumReleaseAgeExcludes')
	} — a fresh release is otherwise age-gated
${indent}- install from source: ${cyan(`cargo install --git=${repo}/ ${name}`)}
${indent}- file an issue if your platform is unsupported: ${link(issues, issues)}${detail}
`;

	console.error(errorText);

	throw new Error('No prebuilt binary found for the current platform and architecture.');
}
