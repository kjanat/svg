// @ts-check
// Every file outside Cargo's workspace-version mechanism that carries the
// release version. check.mjs verifies these in CI; scripts/release-prepare.ts
// bumps them at release time — one list, no drift between the two.
//
// The pattern's first capture group must be the version string.

/**
 * @typedef {object} Carrier
 * @property {string} file Repo-relative path; missing files are skipped (not every grammar ships every carrier).
 * @property {RegExp} pattern First capture group is the version.
 * @property {boolean} [all] Global pattern: every match must carry the version (e.g. the internal dep requirements in Cargo.toml).
 */

const GRAMMARS = [
	'tree-sitter-svg',
	'tree-sitter-svg-paint',
	'tree-sitter-svg-path',
	'tree-sitter-svg-transform',
];

/** @param {string} grammar @returns {Carrier[]} */
function grammarCarriers(grammar) {
	const dir = `grammars/${grammar}`;
	return [
		{ file: `${dir}/package.json`, pattern: /"version":\s*"([^"]+)"/ },
		{ file: `${dir}/tree-sitter.json`, pattern: /"version":\s*"([^"]+)"/ },
		{ file: `${dir}/Makefile`, pattern: /^VERSION := (.+)$/m },
		{ file: `${dir}/CMakeLists.txt`, pattern: /VERSION "([^"]+)"/ },
		{ file: `${dir}/pyproject.toml`, pattern: /^version\s*=\s*"([^"]+)"/m },
		{ file: `${dir}/build.zig.zon`, pattern: /\.version = "([^"]+)"/ },
		{ file: `${dir}/pom.xml`, pattern: /<version>(\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?)<\/version>/ },
	];
}

/** @type {Carrier[]} */
export const carriers = [
	...GRAMMARS.flatMap(grammarCarriers),
	{ file: 'editors/zed-svg/extension.toml', pattern: /^version\s*=\s*"([^"]+)"/m },
	{ file: 'tree-sitter.json', pattern: /"version":\s*"([^"]+)"/ },
	// Internal workspace dep requirements: crates.io requires explicit
	// versions on path deps, and a stale one publishes crates that pin the
	// previous release. `path = "..."` sits between package and version, so
	// the pattern must not assume adjacency.
	{
		file: 'Cargo.toml',
		pattern: /= \{ package = "(?:svg-|tree-sitter-svg)[\w-]*",[^}\n]*?version = "([^"]+)"/g,
		all: true,
	},
];

/**
 * The workspace version in the root Cargo.toml — the single source of truth.
 *
 * @param {string} cargoTomlSource
 * @returns {string}
 */
export function workspaceVersion(cargoTomlSource) {
	const match = cargoTomlSource.match(/\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m);
	if (!match) {
		throw new Error('Cargo.toml is missing workspace.package.version');
	}
	return match[1];
}
