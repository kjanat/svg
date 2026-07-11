#!/usr/bin/env node
/**
 * Builds npm package trees in `distribution/npm/dist/` for:
 *
 * - every facade package listed in `distribution/npm/targets.json` (one per binary)
 * - every per-platform package listed in `distribution/npm/targets.json`
 *
 * Native binary tarballs are read from `distribution/npm/downloads/` by default. CI usually
 * populates that directory with `gh release download`. Outside CI (no
 * `GITHUB_ACTIONS=true`), a dev machine only ever has native binaries for its
 * own host, so a bare local run: builds the host's own tarball with
 * `cargo build --release` if it's missing, and treats every other target's
 * missing tarball as skippable (same as `--skip-missing`) instead of failing.
 *
 * Usage:
 *
 *   node distribution/npm/scripts/build-packages.ts # version from Cargo.toml
 *   node distribution/npm/scripts/build-packages.ts --only=linux-x64-gnu
 *   node distribution/npm/scripts/build-packages.ts --version 0.0.0-dev
 *   node distribution/npm/scripts/build-packages.ts --downloads=/tmp/artifacts
 */
/// <reference types="node" />
import { cli, command, flag } from '@kjanat/dreamcli';
import { blue, green, italic, magenta, underline } from 'ansispeck';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { cp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join, posix, resolve } from 'node:path';
import { env, stdout } from 'node:process';
import { promisify } from 'node:util';
import { gunzip } from 'node:zlib';

const gunzipAsync = promisify(gunzip);

const npmDir = resolve(import.meta.dirname, '..');
const repoDir = resolve(npmDir, '..', '..');
const distDir = join(npmDir, 'dist');

const BLOCK_SIZE = 512;
const ARCHIVE_PREFIX = 'svg';
const META_PACKAGE = 'svg-language-server';
const FACADE_LIB_FILES = ['resolve.mjs', 'launch.mjs'] as const;

interface CargoManifest {
	name: string;
	version: string;
	license?: string;
	homepage?: string;
	repository?: string;
	authors: string[];
}

/**
 * Read the workspace's Cargo metadata and return the manifest of the package
 * every npm artifact inherits its metadata from ([`META_PACKAGE`]).
 *
 * The workspace is virtual, so there is no root package to prefer; the LSP
 * server crate carries the same workspace-inherited version/license/authors
 * as every other publishable member.
 */
function readCargoManifest(): CargoManifest {
	const result = spawnSync('cargo', ['metadata', '--no-deps', '--format-version', '1'], {
		cwd: repoDir,
		encoding: 'utf8',
		// metadata output for a workspace can dwarf the 1 MiB Node default.
		maxBuffer: 64 * 1024 * 1024,
	});
	if (result.status !== 0) {
		const err = (result.stderr || '').trim();
		throw new Error(`cargo metadata failed${err ? `: ${err}` : ''}`);
	}
	const envelope = JSON.parse(result.stdout) as { packages?: unknown };
	if (!Array.isArray(envelope.packages) || envelope.packages.length === 0) {
		throw new Error('cargo metadata produced unexpected shape (no packages)');
	}
	const pkg = envelope.packages.find(
		(p): p is Record<string, unknown> =>
			typeof p === 'object' && p !== null && !Array.isArray(p) && (p as Record<string, unknown>).name === META_PACKAGE,
	);
	if (!pkg) {
		throw new Error(`cargo metadata has no package named ${META_PACKAGE}`);
	}
	if (typeof pkg.name !== 'string' || typeof pkg.version !== 'string') {
		throw new Error('cargo metadata produced unexpected shape (missing name/version)');
	}
	return {
		name: pkg.name,
		version: pkg.version,
		license: typeof pkg.license === 'string' ? pkg.license : undefined,
		homepage: typeof pkg.homepage === 'string' ? pkg.homepage : undefined,
		repository: typeof pkg.repository === 'string' ? pkg.repository : undefined,
		authors: Array.isArray(pkg.authors) ? pkg.authors.filter((a): a is string => typeof a === 'string') : [],
	};
}

/**
 * npm package fields shared by every facade and platform package, derived
 * from the Cargo manifest so Cargo stays the single source of truth.
 */
function packageMetadata(manifest: CargoManifest): Record<string, unknown> {
	const out: Record<string, unknown> = {};
	if (manifest.license) out.license = manifest.license;
	if (manifest.authors[0]) out.author = manifest.authors[0];
	if (manifest.homepage) out.homepage = manifest.homepage;
	if (manifest.repository) {
		out.repository = { type: 'git', url: `git+${manifest.repository}.git` };
		out.bugs = { url: `${manifest.repository}/issues` };
	}
	return out;
}

type Libc = 'glibc' | 'musl';

interface Target {
	pkg: string;
	rust: string;
	os: NodeJS.Platform[];
	cpu: NodeJS.Architecture[];
	libc?: Libc[];
	runner: string;
	build: 'cargo' | 'cross' | 'cargo-cross-toolchain' | 'cargo-build-std' | 'vm';
	tier: 1 | 2 | 3;
	experimental?: boolean;
}

interface Facade {
	name: string;
	bin: string;
	description?: string;
}

interface Matrix {
	scope: string;
	binaries: string[];
	facades: Facade[];
	targets: Target[];
}

interface BuildOptions {
	version: string;
	only: Set<string> | null;
	skipMissing: boolean;
	downloadsDir: string;
	/**
	 * `true` outside CI: a missing tarball for a *non-host* target is treated
	 * as skippable, and a missing tarball for the *host* target is built on
	 * demand instead of failing. `false` in CI, where every target's tarball
	 * is expected to already exist via `gh release download`.
	 */
	local: boolean;
	/** This machine's Rust target triple, or `null` if `rustc` isn't on `PATH`. */
	hostTriple: string | null;
}

interface TarEntry {
	name: string;
	size: number;
	type: string;
	bodyOffset: number;
}

function errorMessage(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

function errorCode(error: unknown): string | undefined {
	if (error instanceof Error && 'code' in error && typeof error.code === 'string') {
		return error.code;
	}
	return undefined;
}

function isCi(): boolean {
	return env.GITHUB_ACTIONS === 'true';
}

/**
 * Run `fn` inside a GitHub Actions log group when executing under Actions,
 * emitting `::endgroup::` even on throw so per-package builds collapse
 * cleanly without altering local-dev output.
 */
async function withLogGroup<T>(title: string, fn: () => Promise<T>): Promise<T> {
	const inActions = isCi();
	if (inActions) stdout.write(`::group::${title}\n`);
	try {
		return await fn();
	} finally {
		if (inActions) stdout.write('::endgroup::\n');
	}
}

function hostRustTriple(): string | null {
	const result = spawnSync('rustc', ['--print', 'host-tuple'], { encoding: 'utf8' });
	return result.status === 0 ? result.stdout.trim() : null;
}

function readMatrix(): Matrix {
	const path = join(npmDir, 'targets.json');
	const matrix = JSON.parse(readFileSync(path, 'utf8')) as Matrix;
	for (const facade of matrix.facades) {
		if (!matrix.binaries.includes(facade.bin)) {
			throw new Error(`facade ${facade.name} references bin ${facade.bin}, not in binaries[]`);
		}
	}
	return matrix;
}

// Loaded at module scope: the --only enum needs the package names before the
// command definition exists.
const matrix = readMatrix();
const [firstPackageName, ...restPackageNames] = [
	...matrix.targets.map((t) => t.pkg),
	...matrix.facades.map((f) => f.name),
];
if (!firstPackageName) {
	throw new Error('targets.json defines no packages');
}

async function cleanDist(): Promise<void> {
	await rm(distDir, { recursive: true, force: true });
	await mkdir(distDir, { recursive: true });
}

/**
 * Build one facade package from its checked-in template plus the shared
 * launcher lib. The bin shim is generated so the template dir stays free of
 * per-binary boilerplate.
 */
async function buildFacade(
	matrix: Matrix,
	facade: Facade,
	version: string,
	builtTargets: Target[],
	meta: Record<string, unknown>,
): Promise<void> {
	const templateDir = join(npmDir, 'facade', facade.name);
	const template = JSON.parse(await readFile(join(templateDir, 'package.json'), 'utf8')) as Record<string, unknown>;

	const dest = join(distDir, facade.name);
	await mkdir(join(dest, 'bin'), { recursive: true });
	await mkdir(join(dest, 'lib'), { recursive: true });

	// Cargo metadata wins over the template for the fields it owns (license,
	// author, homepage, repository, bugs) so there's one source of truth.
	const packageJson = {
		...template,
		...meta,
		version,
		optionalDependencies: Object.fromEntries(
			builtTargets.map((target) => [`${matrix.scope}/${target.pkg}`, version]),
		),
	};

	await writeJson(join(dest, 'package.json'), packageJson);
	await cp(join(templateDir, 'README.md'), join(dest, 'README.md'));
	await cp(join(repoDir, 'LICENSE'), join(dest, 'LICENSE'));

	for (const file of FACADE_LIB_FILES) {
		await cp(join(npmDir, 'facade', 'lib', file), join(dest, 'lib', file));
	}
	await writeFile(
		join(dest, 'bin', `${facade.bin}.mjs`),
		`\
#!/usr/bin/env node
import launch from "#launch";
launch("${facade.bin}");
`,
		{ mode: 0o755 },
	);

	console.log(`built ${formatPackage(undefined, facade.name, version)}`);
}

/**
 * Build the host's own release binaries and pack them into the tarball
 * `buildPlatformPackage` expects, so a bare local `build-packages` run works
 * without a prior `gh release download`.
 */
async function buildHostTarball(
	matrix: Matrix,
	target: Target,
	version: string,
	downloadsDir: string,
): Promise<void> {
	console.log(`→ no tarball for host target ${target.rust}; building it with \`cargo build --release\``);

	const build = spawnSync('cargo', ['build', '--release', '--workspace'], { cwd: repoDir, stdio: 'inherit' });
	if (build.status !== 0) {
		throw new Error(`cargo build --release failed while building the host tarball for ${target.rust}`);
	}

	const releaseDir = join(repoDir, 'target', 'release');
	const fileNames = matrix.binaries.map((name) => (target.os.includes('win32') ? `${name}.exe` : name));

	await mkdir(downloadsDir, { recursive: true });
	const tarball = tarballPath(downloadsDir, version, target);
	const packed = spawnSync('tar', ['czf', tarball, '-C', releaseDir, ...fileNames]);
	if (packed.status !== 0) {
		throw new Error(
			`tar failed while packing the host tarball for ${target.rust}: ${(packed.stderr ?? '').toString().trim()}`,
		);
	}
}

/**
 * Build a platform package by extracting the release tarball's binaries into
 * `distribution/npm/dist/<target.pkg>/bin/`.
 *
 * @returns The provided `target` when built, or `null` when skipped
 *   (missing tarball honoring `--skip-missing`, tier 3, or a local non-host run).
 */
async function buildPlatformPackage(
	matrix: Matrix,
	target: Target,
	opts: BuildOptions,
	meta: Record<string, unknown>,
): Promise<Target | null> {
	const packageName = `${matrix.scope}/${target.pkg}`;
	const dest = join(distDir, target.pkg);
	const tarball = tarballPath(opts.downloadsDir, opts.version, target);
	const maySkip = opts.skipMissing || target.tier === 3 || opts.local;
	const isHost = opts.local && opts.hostTriple === target.rust;

	await mkdir(join(dest, 'bin'), { recursive: true });

	let binaries: Map<string, Buffer>;

	try {
		binaries = await extractBinariesFromTarball(tarball, matrix.binaries);
	} catch (error) {
		if (isHost && errorCode(error) === 'ENOENT') {
			await buildHostTarball(matrix, target, opts.version, opts.downloadsDir);
			binaries = await extractBinariesFromTarball(tarball, matrix.binaries);
		} else if (maySkip) {
			console.warn(`skipping ${packageName}: ${errorCode(error) ?? errorMessage(error)}`);
			await removePartialPackage(dest);
			return null;
		} else {
			throw new Error(`failed to read ${tarball}: ${errorMessage(error)}`);
		}
	}

	const missing = await writePlatformBinaries(dest, matrix.binaries, target, binaries);

	if (missing) {
		if (maySkip) {
			console.warn(`skipping ${packageName}: missing ${missing} in archive`);
			await removePartialPackage(dest);
			return null;
		}
		throw new Error(`missing ${missing} in ${tarball}`);
	}

	const pkg = platformPackageJson(matrix, target, opts.version, meta);
	await writeJson(join(dest, 'package.json'), pkg);

	const readme = platformReadme(matrix, target);
	await writeFile(join(dest, 'README.md'), readme);

	await cp(join(repoDir, 'LICENSE'), join(dest, 'LICENSE'));

	console.log(`built ${formatPackage(matrix.scope, target.pkg, opts.version)}`);
	return target;
}

/**
 * @returns The missing filename not found in `binaries`, or `null` if all were written.
 */
async function writePlatformBinaries(
	dest: string,
	binaryNames: string[],
	target: Target,
	binaries: Map<string, Buffer>,
): Promise<string | null> {
	for (const binaryName of binaryNames) {
		const fileName = target.os.includes('win32') ? `${binaryName}.exe` : binaryName;
		const data = binaries.get(fileName);

		if (!data) return fileName;

		await writeFile(join(dest, 'bin', fileName), data, { mode: 0o755 });
	}

	return null;
}

function platformPackageJson(
	matrix: Matrix,
	target: Target,
	version: string,
	meta: Record<string, unknown>,
): Record<string, unknown> {
	const keywords = [
		...new Set([
			'svg',
			'prebuilt',
			'binary',
			'native',
			'lsp',
			'linter',
			'formatter',
			...target.os,
			...target.cpu,
			...(target.libc ?? []),
		]),
	];
	return {
		name: `${matrix.scope}/${target.pkg}`,
		version,
		description: `Prebuilt ${matrix.binaries.join(' + ')} ${target.rust} binaries; selected automatically by npm, also runnable standalone via npx.`,
		keywords,
		...meta,
		os: target.os,
		cpu: target.cpu,
		...(target.libc ? { libc: target.libc } : {}),
		// npm chmods bin targets at link time; a `directories.bin` alongside `bin` is an error.
		bin: Object.fromEntries(
			matrix.binaries.map((name) => [name, `bin/${name}${target.os.includes('win32') ? '.exe' : ''}`]),
		),
		exports: { './package.json': './package.json' },
		files: ['bin/'],
	};
}

function platformReadme(matrix: Matrix, target: Target): string {
	const packageName = `${matrix.scope}/${target.pkg}`;
	const binaries = matrix.binaries.map((name) => `\`${name}\``).join(', ');
	const platform = [...target.os, ...target.cpu, ...(target.libc ? [target.libc] : [])].join(' · ');
	const facadeList = matrix.facades.map((f) => `[\`${f.name}\`](https://npm.im/${f.name})`).join(', ');

	return `\
# ${packageName}

Prebuilt ${binaries} binaries for **${platform}** (rustc target \`${target.rust}\`).
The platform-specific package shared by ${facadeList}.

## Do I install this?

Usually not. Install one of the main packages and npm picks the matching binary
for your platform automatically:

\`\`\`sh
npm install ${matrix.facades[0]?.name}
\`\`\`

This package is listed in each facade's \`optionalDependencies\`. npm resolves the one
whose \`os\`/\`cpu\`${target.libc ? '/`libc`' : ''} matches your machine and skips the rest. Depending on it
directly pins you to a single platform, so prefer a facade for anything portable.

## Standalone use

The package is a working CLI in its own right — its bins point straight at the
bundled binaries, so on a matching machine it runs without a facade:

\`\`\`sh
npx --package ${packageName} svg-lint icon.svg
npx --package ${packageName} svg-format icon.svg
\`\`\`

## Contents

- ${binaries}: prebuilt native binaries under \`bin/\`.
- No dependencies, no install scripts, no network access.

Released under the same license as the facades (see \`LICENSE\`).
`;
}

async function extractBinariesFromTarball(
	tarballPath: string,
	binaryNames: string[],
): Promise<Map<string, Buffer>> {
	const compressed = await readFile(tarballPath);
	const tar = await gunzipAsync(compressed);

	const wanted = new Set([
		...binaryNames,
		...binaryNames.map((name) => `${name}.exe`),
	]);

	const found = new Map<string, Buffer>();

	for (const entry of readTarEntries(tar)) {
		if (!isRegularTarFile(entry.type)) continue;

		const fileName = posix.basename(entry.name);
		if (!wanted.has(fileName)) continue;

		found.set(fileName, Buffer.from(tar.subarray(entry.bodyOffset, entry.bodyOffset + entry.size)));
	}

	return found;
}

/**
 * Parse an uncompressed tar buffer into entry metadata. Parsing stops at the
 * tar end marker (a zero block).
 *
 * @throws Error if an entry's declared size extends past the end of the buffer.
 */
function readTarEntries(tar: Buffer): TarEntry[] {
	const entries: TarEntry[] = [];

	let offset = 0;

	while (offset + BLOCK_SIZE <= tar.length) {
		const header = tar.subarray(offset, offset + BLOCK_SIZE);

		if (isZeroBlock(header)) break;

		const name = readTarPath(header);
		const size = readTarSize(header);
		const type = readTarString(header, 156, 1) || '0';
		const bodyOffset = offset + BLOCK_SIZE;
		const nextOffset = bodyOffset + alignToBlock(size);

		if (nextOffset > tar.length) {
			throw new Error(`malformed tar archive: ${name} extends past end of file`);
		}

		entries.push({ name, size, type, bodyOffset });
		offset = nextOffset;
	}

	return entries;
}

function readTarPath(header: Buffer): string {
	const name = readTarString(header, 0, 100);
	const prefix = readTarString(header, 345, 155);

	return prefix ? `${prefix}/${name}` : name;
}

function readTarSize(header: Buffer): number {
	const raw = readTarString(header, 124, 12).trim();

	if (!raw) return 0;

	const size = Number.parseInt(raw, 8);

	if (!Number.isFinite(size)) {
		throw new Error(`malformed tar archive: invalid size ${JSON.stringify(raw)}`);
	}

	return size;
}

function readTarString(buffer: Buffer, start: number, length: number): string {
	const bytes = buffer.subarray(start, start + length);
	const end = bytes.indexOf(0);
	const slice = end === -1 ? bytes : bytes.subarray(0, end);

	return slice.toString('utf8');
}

function isZeroBlock(block: Buffer): boolean {
	return block.every((byte) => byte === 0);
}

function isRegularTarFile(type: string): boolean {
	return type === '0' || type === '\0';
}

function alignToBlock(size: number): number {
	return Math.ceil(size / BLOCK_SIZE) * BLOCK_SIZE;
}

function tarballPath(downloadsDir: string, version: string, target: Target): string {
	const tag = version.startsWith('v') ? version : `v${version}`;
	return join(downloadsDir, `${ARCHIVE_PREFIX}-${tag}-${target.rust}.tar.gz`);
}

async function writeJson(path: string, value: unknown): Promise<void> {
	await writeFile(path, `${JSON.stringify(value, null, 2)}\n`);
}

async function removePartialPackage(path: string): Promise<void> {
	await rm(path, { recursive: true, force: true });
}

function formatPackage(scope: string | undefined, name: string, version: string): string {
	const packageName = scope ? `${scope}/${name}` : name;
	return `${blue(underline(packageName))}${green('@')}${magenta(italic(version))}`;
}

/**
 * @throws Error if no platform packages were built (prevents publishing
 *   facades with empty optionalDependencies)
 */
async function build(opts: BuildOptions): Promise<void> {
	const meta = packageMetadata(readCargoManifest());

	await cleanDist();

	const builtTargets: Target[] = [];

	for (const target of matrix.targets) {
		if (opts.only && !opts.only.has(target.pkg)) continue;

		const built = await withLogGroup(
			`${matrix.scope}/${target.pkg}`,
			() => buildPlatformPackage(matrix, target, opts, meta),
		);
		if (built) builtTargets.push(built);
	}

	if (builtTargets.length === 0) {
		throw new Error('no platform packages were built; refusing to publish facades with empty optionalDependencies');
	}

	for (const facade of matrix.facades) {
		if (opts.only && !opts.only.has(facade.name)) continue;
		await withLogGroup(facade.name, () => buildFacade(matrix, facade, opts.version, builtTargets, meta));
	}
}

export const buildPackages = command('build-packages')
	.description('Build the npm package trees in distribution/npm/dist/ from release tarballs')
	.flag('version', flag.string().default(readCargoManifest().version).describe('Version to stamp. Defaults to the Cargo workspace version'))
	.flag('only', flag.array(flag.enum([firstPackageName, ...restPackageNames])).separator(',').unique().describe('Package names to build'))
	.flag('skip-missing', flag.boolean().default(false).describe('Skip targets whose tarball is missing'))
	.flag('downloads', flag.path({ type: 'directory', mustExist: false }).default(join(npmDir, 'downloads')).describe('Tarball directory'))
	.action(async ({ flags }) => {
		await build({
			version: flags.version,
			only: flags.only?.length ? new Set(flags.only) : null,
			skipMissing: flags['skip-missing'],
			downloadsDir: resolve(flags.downloads),
			local: !isCi(),
			hostTriple: hostRustTriple(),
		});
	});

export const app = cli('build-packages').default(buildPackages);

if (import.meta.main) app.run();
