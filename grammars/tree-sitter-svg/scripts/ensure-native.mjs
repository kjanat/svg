#!/usr/bin/env node
// @ts-check
/**
 * Build the native addons this grammar needs and place them where Bun's
 * loader expects them.
 *
 * Two addons matter when a consumer runs under Bun:
 *
 *   1. this grammar's addon            -> prebuilds/<p-a>/tree-sitter-svg.node
 *   2. the `tree-sitter` runtime addon -> prebuilds/<p-a>/tree-sitter.node
 *
 * Node can resolve `build/Release/*.node` through node-gyp-build. Bun's
 * CommonJS loader expects the prebuild layout, and Bun does not run a
 * dependency's own install script for the optional tree-sitter peer. This
 * script bridges that gap without making tree-sitter a hard dependency.
 */
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { arch, execPath, exit, platform } from 'node:process';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const platformDir = `${platform}-${arch}`;

const bundledNodeGyp = join(
	dirname(dirname(execPath)),
	'lib',
	'node_modules',
	'npm',
	'node_modules',
	'node-gyp',
	'bin',
	'node-gyp.js',
);

/**
 * Locate a runnable `node-gyp.js`, preferring the copy bundled with npm.
 * @returns {string | null} Path to `node-gyp.js`, or `null` when it must be resolved from `PATH`.
 */
function resolveNodeGyp() {
	if (existsSync(bundledNodeGyp)) {
		return bundledNodeGyp;
	}
	try {
		return require.resolve('node-gyp/bin/node-gyp.js');
	} catch {
		return null;
	}
}

/**
 * Run `node-gyp rebuild` in the given package directory.
 * @param {string} cwd Directory of the addon package to build.
 * @returns {void}
 */
function runNodeGyp(cwd) {
	const nodeGyp = resolveNodeGyp();
	if (nodeGyp) {
		execFileSync(execPath, [nodeGyp, 'rebuild'], { cwd, stdio: 'inherit' });
		return;
	}
	execFileSync('node-gyp', ['rebuild'], { cwd, stdio: 'inherit' });
}

/**
 * Build an addon (if not already built) and copy it into the prebuild layout.
 * @param {string} pkgDir Package root containing `binding.gyp`/`build`.
 * @param {string} builtName File name node-gyp emits under `build/Release`.
 * @param {string} prebuildName Base name (no extension) for the staged prebuild.
 * @returns {void}
 */
function ensureAddon(pkgDir, builtName, prebuildName) {
	const built = join(pkgDir, 'build', 'Release', builtName);
	if (!existsSync(built)) {
		runNodeGyp(pkgDir);
	}
	if (!existsSync(built)) {
		throw new Error(`node-gyp did not produce ${built}`);
	}

	const prebuild = join(pkgDir, 'prebuilds', platformDir, `${prebuildName}.node`);
	mkdirSync(dirname(prebuild), { recursive: true });
	copyFileSync(built, prebuild);
}

/**
 * Resolve the install root of an optional dependency, if present.
 * @param {string} packageName Package to resolve.
 * @returns {string | null} The package root directory, or `null` when not installed.
 */
function optionalPackageRoot(packageName) {
	try {
		return dirname(require.resolve(`${packageName}/package.json`));
	} catch {
		return null;
	}
}

/**
 * Build this grammar's addon plus, when present, the shared `tree-sitter` runtime addon.
 * @returns {void}
 */
function buildNativeAddons() {
	const grammarRoot = fileURLToPath(new URL('..', import.meta.url));
	ensureAddon(grammarRoot, 'tree_sitter_svg_binding.node', 'tree-sitter-svg');

	const treeSitterRoot = optionalPackageRoot('tree-sitter');
	if (treeSitterRoot) {
		ensureAddon(treeSitterRoot, 'tree_sitter_runtime_binding.node', 'tree-sitter');
	}
}

try {
	buildNativeAddons();
} catch (error) {
	// execFileSync (node-gyp) and the "did not produce" guard both throw here;
	// surface an actionable message instead of a bare Node stack trace.
	const reason = error instanceof Error ? error.message : String(error);
	console.error(`ensure-native: could not build the native addon: ${reason}`);
	console.error(
		'A C toolchain and node-gyp are required (e.g. build-essential / Xcode CLT plus `node-gyp`).',
	);
	exit(1);
}
