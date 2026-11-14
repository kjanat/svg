#!/usr/bin/env node
// @ts-check
/**
 * Build this grammar's native addon and stage it where Bun's loader expects it:
 *
 *   prebuilds/<platform>-<arch>/tree-sitter-svg-path.node
 *
 * Node resolves `build/Release/*.node` through node-gyp-build; Bun's CommonJS
 * loader expects the prebuild layout. This bridges that gap. The shared
 * `tree-sitter` runtime addon is built by the host grammar's ensure-native (or
 * tree-sitter's own install), so this only builds the path sub-grammar's addon.
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

try {
	const grammarRoot = fileURLToPath(new URL('..', import.meta.url));
	ensureAddon(grammarRoot, 'tree_sitter_svg_path_binding.node', 'tree-sitter-svg-path');
} catch (error) {
	const reason = error instanceof Error ? error.message : String(error);
	console.error(`ensure-native: could not build the native addon: ${reason}`);
	console.error(
		'A C toolchain and node-gyp are required (e.g. build-essential / Xcode CLT plus `node-gyp`).',
	);
	exit(1);
}
