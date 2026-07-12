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
import { Parser, type ReadEntry } from 'tar';

const npmDir = resolve(import.meta.dirname, '..');
const repoDir = resolve(npmDir, '..', '..');
const distDir = join(npmDir, 'dist');

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
	/** Additional npm names this facade publishes under (byte-identical twins). */
	alsoPublishAs?: string[];
	/** Unscoped alias package: exact-pinned dependency + bin shims deferring to this facade. */
	shim?: string;
	/** Template dir under `facade/`; defaults to `name` (required for scoped names). */
	template?: string;
	/** Tool prefix for this facade's platform sub-packages: `<scope>/<pkg>-<target.pkg>`. */
	pkg: string;
	bin: string;
	description?: string;
}

interface Bundle {
	name: string;
	template: string;
	description?: string;
}

interface Matrix {
	scope: string;
	binaries: string[];
	facades: Facade[];
	bundle?: Bundle;
	targets: Target[];
}

/**
 * Directory name for a package inside `dist/`, npm-pack style: the scope `@`
 * is dropped and `/` becomes `-`, so scoped names stay valid path segments
 * (`@svg-toolkit/svg-lint` → `svg-toolkit-svg-lint`).
 */
function dirName(packageName: string): string {
	return packageName.replace(/^@/, '').replace(/\//g, '-');
}

/** All npm names a facade publishes under, primary first. */
function publishNames(facade: Facade): string[] {
	return [facade.name, ...(facade.alsoPublishAs ?? [])];
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
	...matrix.facades.flatMap((f) => (f.shim ? [f.shim] : [])),
	...(matrix.bundle ? [matrix.bundle.name] : []),
];
if (!firstPackageName) {
	throw new Error('targets.json defines no packages');
}

async function cleanDist(): Promise<void> {
	await rm(distDir, { recursive: true, force: true });
	await mkdir(distDir, { recursive: true });
}

/**
 * The crate whose README documents this facade's tool (features, rules,
 * configuration). The facade's published README is composed from the npm
 * template head plus that crate README's sections, so the npm pages carry
 * the full documentation without a hand-maintained copy that can drift.
 */
function crateDirFor(facade: Facade): string {
	return join(repoDir, 'crates', facade.bin);
}

/**
 * npm README = template head (install/alias specifics) + the crate README
 * from its first `##` section onward, minus the "Part of" footer that only
 * makes sense on GitHub-relative pages.
 */
async function composeFacadeReadme(facade: Facade): Promise<string> {
	const templateDir = join(npmDir, 'facade', facade.template ?? facade.name);
	const head = await readFile(join(templateDir, 'README.md'), 'utf8');

	const crateReadme = await readFile(join(crateDirFor(facade), 'README.md'), 'utf8');
	const firstSection = crateReadme.indexOf('\n## ');
	if (firstSection === -1) {
		throw new Error(`crate README for ${facade.bin} has no sections to compose into the npm README`);
	}
	let body = crateReadme.slice(firstSection + 1);
	// The template head already covers installation; the crate's own Install
	// section (and the GitHub-only "Part of" footer) would duplicate it.
	for (const section of ['## Install\n', '## Part of [svg-language-server]']) {
		const start = body.indexOf(section);
		if (start === -1) continue;
		const next = body.indexOf('\n## ', start + section.length);
		body = next === -1 ? body.slice(0, start) : body.slice(0, start) + body.slice(next + 1);
	}
	body = `${body.trimEnd()}\n`;

	return `${head.trimEnd()}\n\n---\n\n${body}`;
}

/**
 * Build one facade package from its checked-in template plus the shared
 * launcher lib, once per publish name (a scoped twin is byte-identical apart
 * from `package.json.name`). The bin shim is generated so the template dir
 * stays free of per-binary boilerplate.
 */
async function buildFacade(
	matrix: Matrix,
	facade: Facade,
	version: string,
	builtTargets: Target[],
	meta: Record<string, unknown>,
): Promise<void> {
	const templateDir = join(npmDir, 'facade', facade.template ?? facade.name);
	const template = JSON.parse(await readFile(join(templateDir, 'package.json'), 'utf8')) as Record<string, unknown>;
	const readme = await composeFacadeReadme(facade);

	for (const packageName of publishNames(facade)) {
		const dest = join(distDir, dirName(packageName));
		await mkdir(join(dest, 'bin'), { recursive: true });
		await mkdir(join(dest, 'lib'), { recursive: true });

		// Cargo metadata wins over the template for the fields it owns (license,
		// author, homepage, repository, bugs) so there's one source of truth.
		const packageJson = {
			...template,
			...meta,
			name: packageName,
			version,
			optionalDependencies: Object.fromEntries(
				builtTargets.map((target) => [`${matrix.scope}/${facade.pkg}-${target.pkg}`, version]),
			),
		};

		await writeJson(join(dest, 'package.json'), packageJson);
		await writeFile(join(dest, 'README.md'), readme);
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

		console.log(`built ${formatPackage(undefined, packageName, version)}`);
	}
}

/**
 * Build a facade's unscoped alias package: an exact-pinned dependency on the
 * canonical (scoped) facade plus the same bin map, each bin a one-line shim
 * importing the canonical facade's bin script (facades export `./bin/*` for
 * exactly this). Installing the alias counts a download on both names.
 */
async function buildShim(
	facade: Facade,
	shimName: string,
	version: string,
	meta: Record<string, unknown>,
): Promise<void> {
	const templateDir = join(npmDir, 'facade', facade.template ?? facade.name);
	const template = JSON.parse(await readFile(join(templateDir, 'package.json'), 'utf8')) as Record<string, unknown>;
	const readme = await composeFacadeReadme(facade);

	const dest = join(distDir, dirName(shimName));
	await mkdir(join(dest, 'bin'), { recursive: true });

	const packageJson = {
		name: shimName,
		description: template.description,
		keywords: template.keywords,
		repository: template.repository,
		type: 'module',
		// ./bin/* stays importable so the bundle's shims can defer to this one.
		exports: { './package.json': './package.json', './bin/*': './bin/*' },
		bin: template.bin,
		files: ['bin/', 'README.md', 'LICENSE'],
		...meta,
		version,
		dependencies: { [facade.name]: version },
	};

	await writeJson(join(dest, 'package.json'), packageJson);
	await writeFile(join(dest, 'README.md'), readme);
	await cp(join(repoDir, 'LICENSE'), join(dest, 'LICENSE'));

	await writeFile(
		join(dest, 'bin', `${facade.bin}.mjs`),
		`\
#!/usr/bin/env node
import "${facade.name}/bin/${facade.bin}.mjs";
`,
		{ mode: 0o755 },
	);

	console.log(`built ${formatPackage(undefined, shimName, version)}`);
}

/**
 * Build the bundle meta-package: exact-pinned `dependencies` on every
 * facade's user-facing name (the unscoped shim when one exists), plus one
 * generated shim per facade bin that defers to that facade's own shim
 * (facades export `./bin/*` for exactly this).
 */
async function buildBundle(
	matrix: Matrix,
	bundle: Bundle,
	version: string,
	meta: Record<string, unknown>,
): Promise<void> {
	const templateDir = join(npmDir, 'facade', bundle.template);
	const template = JSON.parse(await readFile(join(templateDir, 'package.json'), 'utf8')) as Record<string, unknown>;

	const dest = join(distDir, dirName(bundle.name));
	await mkdir(join(dest, 'bin'), { recursive: true });

	const packageJson = {
		...template,
		...meta,
		name: bundle.name,
		version,
		dependencies: Object.fromEntries(matrix.facades.map((facade) => [facade.shim ?? facade.name, version])),
	};

	await writeJson(join(dest, 'package.json'), packageJson);
	await cp(join(templateDir, 'README.md'), join(dest, 'README.md'));
	await cp(join(repoDir, 'LICENSE'), join(dest, 'LICENSE'));

	for (const facade of matrix.facades) {
		await writeFile(
			join(dest, 'bin', `${facade.bin}.mjs`),
			`\
#!/usr/bin/env node
import "${facade.shim ?? facade.name}/bin/${facade.bin}.mjs";
`,
			{ mode: 0o755 },
		);
	}

	console.log(`built ${formatPackage(undefined, bundle.name, version)}`);
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
 * Extract one target's tarball binaries, or `null` when the tarball is
 * missing/incomplete and that is tolerable (missing tarball honoring
 * `--skip-missing`, tier 3, or a local non-host run). The host target's
 * tarball is built on demand in local runs.
 */
async function extractTargetBinaries(
	matrix: Matrix,
	target: Target,
	opts: BuildOptions,
): Promise<Map<string, Buffer> | null> {
	const tarball = tarballPath(opts.downloadsDir, opts.version, target);
	const maySkip = opts.skipMissing || target.tier === 3 || opts.local;
	const isHost = opts.local && opts.hostTriple === target.rust;

	try {
		return await extractBinariesFromTarball(tarball, matrix.binaries);
	} catch (error) {
		if (isHost && errorCode(error) === 'ENOENT') {
			await buildHostTarball(matrix, target, opts.version, opts.downloadsDir);
			return await extractBinariesFromTarball(tarball, matrix.binaries);
		}
		if (maySkip) {
			console.warn(`skipping ${target.pkg}: ${errorCode(error) ?? errorMessage(error)}`);
			return null;
		}
		throw new Error(`failed to read ${tarball}: ${errorMessage(error)}`);
	}
}

/**
 * Build one facade's platform package for one target by writing its single
 * binary into `distribution/npm/dist/<facade.pkg>-<target.pkg>/bin/`.
 *
 * @returns The provided `target` when built, or `null` when the binary is
 *   missing from the archive and skipping is tolerable.
 */
async function buildPlatformPackage(
	matrix: Matrix,
	facade: Facade,
	target: Target,
	binaries: Map<string, Buffer>,
	opts: BuildOptions,
	meta: Record<string, unknown>,
): Promise<Target | null> {
	const pkgSuffix = `${facade.pkg}-${target.pkg}`;
	const packageName = `${matrix.scope}/${pkgSuffix}`;
	const dest = join(distDir, pkgSuffix);
	const maySkip = opts.skipMissing || target.tier === 3 || opts.local;

	const fileName = target.os.includes('win32') ? `${facade.bin}.exe` : facade.bin;
	const data = binaries.get(fileName);
	if (!data) {
		if (maySkip) {
			console.warn(`skipping ${packageName}: missing ${fileName} in archive`);
			return null;
		}
		throw new Error(`missing ${fileName} in the ${target.rust} archive`);
	}

	await mkdir(join(dest, 'bin'), { recursive: true });
	await writeFile(join(dest, 'bin', fileName), data, { mode: 0o755 });

	const pkg = platformPackageJson(matrix, facade, target, opts.version, meta);
	await writeJson(join(dest, 'package.json'), pkg);

	const readme = platformReadme(matrix, facade, target);
	await writeFile(join(dest, 'README.md'), readme);

	await cp(join(repoDir, 'LICENSE'), join(dest, 'LICENSE'));

	console.log(`built ${formatPackage(matrix.scope, pkgSuffix, opts.version)}`);
	return target;
}

function platformPackageJson(
	matrix: Matrix,
	facade: Facade,
	target: Target,
	version: string,
	meta: Record<string, unknown>,
): Record<string, unknown> {
	const keywords = [
		...new Set([
			'svg',
			facade.pkg,
			'prebuilt',
			'binary',
			'native',
			...target.os,
			...target.cpu,
			...(target.libc ?? []),
		]),
	];
	return {
		name: `${matrix.scope}/${facade.pkg}-${target.pkg}`,
		version,
		description: `Prebuilt ${facade.bin} ${target.rust} binary; selected automatically by npm, also runnable standalone via npx.`,
		keywords,
		...meta,
		os: target.os,
		cpu: target.cpu,
		...(target.libc ? { libc: target.libc } : {}),
		// npm chmods bin targets at link time; a `directories.bin` alongside `bin` is an error.
		bin: { [facade.bin]: `bin/${facade.bin}${target.os.includes('win32') ? '.exe' : ''}` },
		exports: { './package.json': './package.json' },
		files: ['bin/'],
	};
}

function platformReadme(matrix: Matrix, facade: Facade, target: Target): string {
	const packageName = `${matrix.scope}/${facade.pkg}-${target.pkg}`;
	const platform = [...target.os, ...target.cpu, ...(target.libc ? [target.libc] : [])].join(' · ');

	return `\
# ${packageName}

Prebuilt \`${facade.bin}\` binary for **${platform}** (rustc target \`${target.rust}\`).
The platform-specific package of [\`${facade.name}\`](https://npm.im/${facade.name}).

## Do I install this?

Usually not. Install the main package and npm picks the matching binary
for your platform automatically:

\`\`\`sh
npm install ${facade.name}
\`\`\`

This package is listed in \`${facade.name}\`'s \`optionalDependencies\`. npm resolves the one
whose \`os\`/\`cpu\`${target.libc ? '/`libc`' : ''} matches your machine and skips the rest. Depending on it
directly pins you to a single platform, so prefer \`${facade.name}\` for anything portable.

## Standalone use

The package is a working CLI in its own right — its bin points straight at the
bundled binary, so on a matching machine it runs without the facade:

\`\`\`sh
npx --package ${packageName} ${facade.bin} --version
\`\`\`

## Contents

- \`${facade.bin}\`: prebuilt native binary under \`bin/\`.
- No dependencies, no install scripts, no network access.

Released under the same license as \`${facade.name}\` (see \`LICENSE\`).
`;
}

async function extractBinariesFromTarball(
	tarballPath: string,
	binaryNames: string[],
): Promise<Map<string, Buffer>> {
	const compressed = await readFile(tarballPath);

	const wanted = new Set([
		...binaryNames,
		...binaryNames.map((name) => `${name}.exe`),
	]);

	const found = new Map<string, Buffer>();

	// node-tar handles gzip, GNU longname/longlink, PAX headers, and base-256
	// sizes — formats the archives may grow into that a hand-rolled ustar
	// parser would silently mis-read.
	await new Promise<void>((resolvePromise, reject) => {
		const parser = new Parser({
			onReadEntry: (entry: ReadEntry) => {
				const fileName = posix.basename(entry.path);
				if (entry.type !== 'File' || !wanted.has(fileName)) {
					entry.resume();
					return;
				}
				const chunks: Buffer[] = [];
				entry.on('data', (chunk: Buffer) => chunks.push(chunk));
				entry.on('end', () => {
					found.set(fileName, Buffer.concat(chunks));
				});
			},
		});
		parser.on('error', reject);
		parser.on('end', resolvePromise);
		parser.end(compressed);
	});

	return found;
}

function tarballPath(downloadsDir: string, version: string, target: Target): string {
	const tag = version.startsWith('v') ? version : `v${version}`;
	return join(downloadsDir, `${ARCHIVE_PREFIX}-${tag}-${target.rust}.tar.gz`);
}

async function writeJson(path: string, value: unknown): Promise<void> {
	await writeFile(path, `${JSON.stringify(value, null, 2)}\n`);
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

	// A platform pkg (facade × target) is selected by naming either side in
	// --only: the target platform (all tools for it) or the facade (all its
	// platforms). Facade packages themselves are only built when named.
	const builtByFacade = new Map<string, Target[]>(matrix.facades.map((f) => [f.name, []]));

	for (const target of matrix.targets) {
		const selectedFacades = matrix.facades.filter(
			(f) => !opts.only || opts.only.has(target.pkg) || opts.only.has(f.name),
		);
		if (selectedFacades.length === 0) continue;

		const binaries = await extractTargetBinaries(matrix, target, opts);
		if (!binaries) continue;

		for (const facade of selectedFacades) {
			const built = await withLogGroup(
				`${matrix.scope}/${facade.pkg}-${target.pkg}`,
				() => buildPlatformPackage(matrix, facade, target, binaries, opts, meta),
			);
			if (built) builtByFacade.get(facade.name)?.push(built);
		}
	}

	if ([...builtByFacade.values()].every((targets) => targets.length === 0)) {
		throw new Error('no platform packages were built; refusing to publish facades with empty optionalDependencies');
	}

	for (const facade of matrix.facades) {
		if (opts.only && !opts.only.has(facade.name)) continue;
		const builtTargets = builtByFacade.get(facade.name) ?? [];
		if (builtTargets.length === 0) {
			throw new Error(`no platform packages built for ${facade.name}; refusing a facade with empty optionalDependencies`);
		}
		await withLogGroup(facade.name, () => buildFacade(matrix, facade, opts.version, builtTargets, meta));
	}

	for (const facade of matrix.facades) {
		const shim = facade.shim;
		if (!shim) continue;
		if (opts.only && !opts.only.has(shim) && !opts.only.has(facade.name)) continue;
		await withLogGroup(shim, () => buildShim(facade, shim, opts.version, meta));
	}

	if (matrix.bundle && (!opts.only || opts.only.has(matrix.bundle.name))) {
		const bundle = matrix.bundle;
		await withLogGroup(bundle.name, () => buildBundle(matrix, bundle, opts.version, meta));
	}
}

export const buildPackages = command('build-packages')
	.description('Build the npm package trees in distribution/npm/dist/ from release tarballs')
	.flag('version', flag.string().default(readCargoManifest().version).describe('Version to stamp. Defaults to the Cargo workspace version'))
	.flag('only', flag.array(flag.enum([firstPackageName, ...restPackageNames])).separator(',').unique().describe('Platform or facade names to build'))
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
