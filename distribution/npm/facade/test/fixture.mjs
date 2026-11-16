import { chmodSync, cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import process from 'node:process';

const libDir = join(import.meta.dirname, '..', 'lib');
export const SCOPE = '@svg-toolkit';
export const TOOL = 'language-server';
export const BIN = 'svg-language-server';
export const EXE = process.platform === 'win32' ? `${BIN}.exe` : BIN;

const ARCH = process.arch === 'arm' ? 'armv7' : process.arch;
const [GNU_ABI, MUSL_ABI] = process.arch === 'arm' ? ['gnueabihf', 'musleabihf'] : ['gnu', 'musl'];
/** This host's glibc and musl package suffixes, meaningful on Linux. */
export const GNU = `linux-${ARCH}-${GNU_ABI}`;
export const MUSL = `linux-${ARCH}-${MUSL_ABI}`;
/** The package suffix this host would install. */
export const HOST = process.platform === 'linux'
	? GNU
	: process.platform === 'win32'
	? `win32-${ARCH}-msvc`
	: `${process.platform}-${ARCH}`;
/** A published target that is never this host. */
export const FOREIGN = HOST === 'netbsd-x64' ? 'freebsd-arm64' : 'netbsd-x64';

const roots = [];

export function removeFixtures() {
	for (const root of roots) rmSync(root, { force: true, recursive: true });
}

/**
 * Throwaway install tree: a facade package with the real lib/ and bin shim, plus platform packages.
 *
 * @param {string[]} installed Target suffixes to materialise, e.g. `linux-x64-gnu`.
 * @param {object} [options]
 * @param {string[]} [options.declared] Suffixes in optionalDependencies; defaults to `installed`.
 * @param {string} [options.script] Body of every installed binary.
 * @param {number} [options.mode] File mode of every installed binary.
 * @param {unknown} [options.repository] The facade manifest's `repository`.
 * @param {unknown} [options.bugs] The facade manifest's `bugs`.
 */
export function makeFixture(installed, options = {}) {
	const {
		declared = installed,
		script = '#!/bin/sh\nexit 0\n',
		mode = 0o755,
		repository = { url: 'git+https://github.com/kjanat/svg.git' },
		bugs = { url: 'https://github.com/kjanat/svg/issues' },
	} = options;
	const root = mkdtempSync(join(tmpdir(), 'svg-facade-'));
	roots.push(root);

	mkdirSync(join(root, 'lib'), { recursive: true });
	for (const file of ['resolve.mjs', 'launch.mjs', 'libc.mjs']) {
		cpSync(join(libDir, file), join(root, 'lib', file));
	}
	mkdirSync(join(root, 'bin'), { recursive: true });
	writeFileSync(join(root, 'bin', `${BIN}.mjs`), `#!/usr/bin/env node\nimport launch from '#launch';\nlaunch('${BIN}');\n`);

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
			repository,
			bugs,
			optionalDependencies: Object.fromEntries(
				declared.map((suffix) => [`${SCOPE}/${TOOL}-${suffix}`, '0.0.0-test']),
			),
		}),
	);

	// ansispeck is a real dependency of the facade; a pass-through stand-in
	// keeps these tests about resolution rather than about colour codes.
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
		writeFileSync(join(pkgDir, 'bin', EXE), script);
		chmodSync(join(pkgDir, 'bin', EXE), mode);
	}

	return root;
}
