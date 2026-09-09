#!/usr/bin/env node
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { Worker } from 'node:worker_threads';
import { detectLibc } from '../../../distribution/npm/facade/lib/libc.mjs';
import {
	archivePath,
	assertExecutable,
	assertVersion,
	digest,
	expectedVersion,
	packageDirectory,
	readJson,
	run,
	runCommand,
	summary,
	targetFor,
	verifyChecksum,
} from './runtime.mjs';

const root = resolve(process.env.NPM_ROOT ?? 'distribution/npm');
const manifest = readJson(join(root, 'targets.json'));
const tag = process.env.RELEASE_TAG;
const version = expectedVersion(tag);
const target = targetFor(manifest, process.env.HOST_PKG);
const evidence = { built: 'not checked', packaged: 'not checked', installed: 'not completed', executed: 'not completed' };
const scratch = mkdtempSync(join(tmpdir(), 'svg-runtime-'));
const packed = new Map();
const installs = [];
let activeStage = 'built';
let commands = 0;
let registry;
let registryUrl;

function checkVersion(label, command, args) {
	assertVersion(runCommand(command, [...args, '--version']), version, label);
	commands++;
	console.log(`PASS ${label}: ${version}`);
}

function bins(pkg) {
	const map = typeof pkg.bin === 'string' ? { [pkg.name.split('/').at(-1)]: pkg.bin } : pkg.bin;
	assert(map && Object.keys(map).length > 0, `${pkg.name}: missing bin commands`);
	return Object.entries(map);
}

function pack(name, directory) {
	if (packed.has(name)) return packed.get(name);
	const source = join(root, 'dist', directory);
	const pkg = readJson(join(source, 'package.json'));
	assert.equal(pkg.name, name);
	assert.equal(pkg.version, version);
	for (const [, file] of bins(pkg)) assertExecutable(join(source, file));
	const output = JSON.parse(runCommand('npm', ['pack', '--json', '--ignore-scripts', '--pack-destination', scratch], { cwd: source }));
	assert.equal(output.length, 1);
	assert.equal(output[0].name, name);
	assert.equal(output[0].version, version);
	const archive = join(scratch, output[0].filename);
	packed.set(name, archive);
	return archive;
}

function install(label, packages, commandsFrom) {
	const app = join(scratch, label);
	mkdirSync(app);
	writeFileSync(join(app, 'package.json'), JSON.stringify({ name: `smoke-${label}`, private: true, version: '0.0.0' }));
	runCommand('npm', [
		'install',
		'--no-audit',
		'--no-fund',
		'--ignore-scripts',
		'--registry',
		registryUrl,
		'--cache',
		join(scratch, 'cache'),
		...packages,
	], { cwd: app });
	for (const name of commandsFrom) {
		const pkg = readJson(join(app, 'node_modules', name, 'package.json'));
		assert.equal(pkg.name, name);
		assert.equal(pkg.version, version);
	}
	installs.push({ label, app, commandsFrom });
}

try {
	assert.equal(process.platform, target.os[0], 'Runtime OS does not match target');
	assert.equal(process.arch, target.cpu[0], 'Runtime architecture does not match target');
	assert(!process.env.SVG_LIBC, 'Runtime checks must use actual libc detection, not SVG_LIBC');
	if (target.libc) assert.equal(detectLibc(), target.libc[0], 'Runtime libc does not match target');
	const archive = archivePath(join(root, 'downloads'), tag, target);
	verifyChecksum(archive);
	const extracted = join(scratch, 'archive');
	mkdirSync(extracted);
	const members = run('tar', ['-tzf', archive]).split(/\r?\n/);
	for (const member of members) assert(!member.startsWith('/') && !member.split('/').includes('..'), `Invalid archive member: ${member}`);
	run('tar', ['-xzf', archive, '-C', extracted]);
	const originals = new Map();
	for (const bin of manifest.binaries) {
		const name = `${bin}${process.platform === 'win32' ? '.exe' : ''}`;
		const matches = members.filter(member => member.split('/').at(-1) === name);
		assert.equal(matches.length, 1, `Expected one ${name} in release archive`);
		const path = join(extracted, matches[0]);
		assertExecutable(path);
		originals.set(bin, path);
	}
	evidence.built = 'archive verified';
	activeStage = 'packaged';
	const platforms = new Map();
	for (const facade of manifest.facades) {
		const name = `${manifest.scope}/${facade.pkg}-${target.pkg}`;
		const directory = `${facade.pkg}-${target.pkg}`;
		const pkg = readJson(join(root, 'dist', directory, 'package.json'));
		assert.deepEqual(pkg.os, target.os);
		assert.deepEqual(pkg.cpu, target.cpu);
		assert.deepEqual(pkg.libc, target.libc);
		for (const [, file] of bins(pkg)) {
			assert.equal(digest(join(root, 'dist', directory, file)), digest(originals.get(facade.bin)), 'Dist binary differs from release archive');
		}
		platforms.set(facade.pkg, pack(name, directory));
		for (const publishedName of [facade.name, ...(facade.alsoPublishAs ?? [])]) pack(publishedName, packageDirectory(publishedName));
		if (facade.shim) pack(facade.shim, packageDirectory(facade.shim));
	}
	if (manifest.bundle) pack(manifest.bundle.name, packageDirectory(manifest.bundle.name));
	evidence.packaged = 'verified';
	activeStage = 'installed';
	const packages = {};
	for (const entry of readdirSync(join(root, 'dist'), { withFileTypes: true })) {
		if (!entry.isDirectory()) continue;
		const pkg = readJson(join(root, 'dist', entry.name, 'package.json'));
		packages[pkg.name] = pkg;
	}
	registry = new Worker(new URL('./registry.mjs', import.meta.url), {
		workerData: { packages, packed: Object.fromEntries(packed), scope: manifest.scope },
	});
	registryUrl = await new Promise((resolveUrl, reject) => {
		registry.once('message', resolveUrl);
		registry.once('error', reject);
	});
	for (const facade of manifest.facades) {
		const platformName = `${manifest.scope}/${facade.pkg}-${target.pkg}`;
		install(`platform-${facade.pkg}`, [platforms.get(facade.pkg)], [platformName]);
		for (const [index, name] of [facade.name, ...(facade.alsoPublishAs ?? [])].entries()) {
			install(`facade-${facade.pkg}-${index}`, [packed.get(name)], [name]);
		}
		if (facade.shim) install(`alias-${facade.pkg}`, [packed.get(facade.shim)], [facade.shim]);
	}
	if (manifest.bundle) install('bundle', [packed.get(manifest.bundle.name)], [manifest.bundle.name]);
	evidence.installed = `${installs.length} clean installs passed`;
	activeStage = 'executed';
	for (const [bin, path] of originals) checkVersion(`archive ${bin}`, path, []);
	for (const { label, app, commandsFrom } of installs) {
		for (const name of commandsFrom) {
			const directory = join(app, 'node_modules', name);
			const pkg = readJson(join(directory, 'package.json'));
			for (const [command, file] of bins(pkg)) {
				const path = join(directory, file);
				assertExecutable(path);
				if (file.endsWith('.mjs')) checkVersion(`${label} direct ${command}`, process.execPath, [path]);
				else {
					assert.equal(digest(path), digest(originals.get(command)), 'Installed raw binary differs from archive');
					checkVersion(`${label} raw ${command}`, path, []);
				}
				const linked = join(app, 'node_modules', '.bin', `${command}${process.platform === 'win32' ? '.cmd' : ''}`);
				assertExecutable(linked);
				// npm may link a platform dependency's identically named command.
				// Direct entry points above cover each facade/alias/bundle; links
				// must still belong to an installed package from this artifact.
				const candidates = Object.keys(packages).flatMap(packageName => {
					const installed = join(app, 'node_modules', packageName);
					if (!existsSync(installed)) return [];
					return bins(readJson(join(installed, 'package.json')))
						.filter(([bin]) => bin === command)
						.map(([, entry]) => ({ relative: `../${packageName}/${entry}`, path: join(installed, entry) }));
				});
				if (process.platform === 'win32') {
					const shim = readFileSync(linked, 'utf8').replaceAll('\\', '/');
					assert(candidates.some(candidate => shim.includes(candidate.relative)), `npm shim does not point into the artifact: ${linked}\n${shim}`);
				} else {assert(
						candidates.some(candidate => realpathSync(candidate.path) === realpathSync(linked)),
						'npm command link does not point into the artifact',
					);}
				checkVersion(`${label} linked ${command}`, linked, []);
			}
		}
		for (const facade of manifest.facades) {
			for (const name of [facade.name, ...(facade.alsoPublishAs ?? [])]) {
				const directory = join(app, 'node_modules', name);
				if (!existsSync(directory)) continue;
				const { resolveBinary } = await import(pathToFileURL(join(directory, 'lib', 'resolve.mjs')).href);
				const selected = resolveBinary(facade.bin);
				const expected = join(
					app,
					'node_modules',
					manifest.scope,
					`${facade.pkg}-${target.pkg}`,
					'bin',
					`${facade.bin}${process.platform === 'win32' ? '.exe' : ''}`,
				);
				assert.equal(realpathSync(selected), realpathSync(expected), 'Facade selected the wrong OS/architecture/libc package');
				assert.equal(digest(selected), digest(originals.get(facade.bin)), 'Installed facade binary differs from release archive');
				for (const other of manifest.targets.filter(t => t.pkg !== target.pkg)) {
					assert(
						!existsSync(join(app, 'node_modules', manifest.scope, `${facade.pkg}-${other.pkg}`)),
						`npm installed an incompatible package: ${other.pkg}`,
					);
				}
			}
		}
	}
	evidence.executed = `${commands} exact-version commands passed`;
} catch (error) {
	evidence[activeStage] = 'FAILED';
	process.exitCode = 1;
	console.error(error);
} finally {
	if (registry) await registry.terminate();
	const report = { tag, target: target.pkg, os: process.platform, arch: process.arch, libc: detectLibc(), ...evidence };
	const reportDir = resolve(process.env.RUNTIME_RESULTS ?? 'runtime-results');
	mkdirSync(reportDir, { recursive: true });
	writeFileSync(join(reportDir, `${target.pkg}.json`), `${JSON.stringify(report, null, 2)}\n`);
	summary(
		`### Runtime evidence: ${target.pkg} (${tag})\n\n| Built | Packaged | Installed | Executed |\n| --- | --- | --- | --- |\n| ${
			Object.values(evidence).join(' | ')
		} |`,
	);
	rmSync(scratch, { recursive: true, force: true });
}
