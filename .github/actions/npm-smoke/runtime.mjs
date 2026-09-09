import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { appendFileSync, existsSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
export const digest = path => createHash('sha256').update(readFileSync(path)).digest('hex');
export const packageDirectory = name => name.replace(/^@/, '').replaceAll('/', '-');

export function run(command, args, options = {}) {
	const result = spawnSync(command, args, { encoding: 'utf8', timeout: 120_000, maxBuffer: 16 * 1024 * 1024, ...options, shell: false });
	return checkedOutput(result, command);
}

function checkedOutput(result, command) {
	if (result.error || result.status !== 0) {
		throw new Error(`${command} failed (${result.status}): ${result.error?.message ?? ''}\n${result.stdout ?? ''}\n${result.stderr ?? ''}`);
	}
	return result.stdout.trim();
}

// cmd.exe is needed for npm's .cmd entry points. Quote each complete argument,
// disable delayed expansion, and reject characters cmd would expand in quotes.
export function cmdLine(command, args) {
	const quote = value => {
		assert(!/["%\r\n]/.test(value), `Unsupported cmd argument: ${value}`);
		return `"${value}"`;
	};
	return `"${[command, ...args].map(quote).join(' ')}"`;
}

let npmPath;
export function runCommand(command, args, options = {}) {
	if (process.platform === 'win32' && command === 'npm') {
		// Resolve an absolute path before entering a package's working directory.
		// Batch wrappers use their own location to find npm-cli.js or npm.exe.
		npmPath ??= run('where.exe', ['npm']).split(/\r?\n/).find(path => /\.(?:cmd|exe)$/i.test(path));
		assert(npmPath, 'No Windows npm command found');
		command = npmPath;
	}
	if (process.platform === 'win32' && command.toLowerCase().endsWith('.cmd')) {
		// Keep shell calls distinct from native argv calls above. Only this
		// branch accepts a command line, quoted and validated by cmdLine.
		const result = spawnSync('cmd.exe', ['/d', '/v:off', '/s', '/c', cmdLine(command, args)], {
			encoding: 'utf8',
			timeout: 120_000,
			maxBuffer: 16 * 1024 * 1024,
			...options,
			shell: false,
			windowsVerbatimArguments: true,
		});
		return checkedOutput(result, command);
	}
	return run(command, args, options);
}

export function expectedVersion(tag) {
	assert(/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(tag), `Invalid release tag: ${tag}`);
	return tag.slice(1);
}

export function assertVersion(output, version, label) {
	assert(output.trim().split(/\s+/).includes(version), `${label}: expected exact version ${version}, got ${output}`);
}

export function assertExecutable(path, platform = process.platform) {
	const stat = statSync(path);
	assert(stat.isFile(), `Not a regular binary: ${path}`);
	if (platform !== 'win32') assert(stat.mode & 0o111, `Executable permission lost: ${path}`);
}

export function runtimeMatrix(manifest) {
	assert.equal(new Set(manifest.targets.map(t => t.pkg)).size, manifest.targets.length, 'Duplicate target package');
	const configured = manifest.targets.filter(t => t.runtime);
	assert(configured.length > 0, 'No required runtime environments configured');
	return configured.map(target => {
		assert(!target.experimental, `${target.pkg}: experimental targets must remain build-only`);
		assert.equal(target.os.length, 1, `${target.pkg}: runtime OS must be unambiguous`);
		assert.equal(target.cpu.length, 1, `${target.pkg}: runtime architecture must be unambiguous`);
		assert(target.runtime.runner, `${target.pkg}: runtime runner missing`);
		if (target.runtime.container) {
			assert(/^node:\d+-alpine$/.test(target.runtime.container), 'Runtime container must be an official Node Alpine image');
			assert(target.os.includes('linux') && target.libc?.includes('musl'), 'Alpine runtime must target Linux musl');
		}
		if (target.libc?.includes('musl')) assert(target.runtime.container, 'musl requires an actual musl container');
		return { pkg: target.pkg, target: target.rust, runner: target.runtime.runner, arch: target.cpu[0], container: target.runtime.container ?? '' };
	});
}

export function targetFor(manifest, pkg) {
	const target = manifest.targets.find(t => t.pkg === pkg);
	assert(target?.runtime, `No runtime configured for ${pkg}`);
	return target;
}

export function archivePath(downloads, tag, target) {
	expectedVersion(tag);
	return join(downloads, `svg-${tag}-${target.rust}.tar.gz`);
}

export function verifyChecksum(archive) {
	const checksum = archive.replace(/\.tar\.gz$/, '.sha256');
	const match = readFileSync(checksum, 'utf8').trim().match(/^([a-fA-F0-9]{64})\s+\*?([^\r\n]+)$/);
	assert(match, `Invalid checksum file: ${checksum}`);
	assert.equal(match[2], archive.split(/[\\/]/).at(-1), 'Checksum names a different archive');
	assert.equal(digest(archive), match[1].toLowerCase(), `Archive checksum mismatch: ${archive}`);
}

export function inventory(manifest, root, tag) {
	const version = expectedVersion(tag);
	return manifest.targets.map(target => {
		const archive = archivePath(join(root, 'downloads'), tag, target);
		const built = existsSync(archive);
		if (built) verifyChecksum(archive);
		const packaged = manifest.facades.every(facade => {
			const path = join(root, 'dist', `${facade.pkg}-${target.pkg}`, 'package.json');
			if (!existsSync(path)) return false;
			const pkg = readJson(path);
			assert.equal(pkg.name, `${manifest.scope}/${facade.pkg}-${target.pkg}`);
			assert.equal(pkg.version, version);
			return existsSync(join(root, 'dist', `${facade.pkg}-${target.pkg}`, 'bin', `${facade.bin}${target.os.includes('win32') ? '.exe' : ''}`));
		});
		if (!target.experimental) assert(built && packaged, `Required target not built/packaged: ${target.pkg}`);
		return `| ${target.pkg} | ${built ? 'archive verified' : 'absent'} | ${packaged ? 'present' : 'absent'} | not run | not run | ${
			target.runtime ? 'required runtime gate' : target.experimental ? 'experimental, non-blocking build only' : 'build only'
		} |`;
	}).join('\n');
}

export function summary(text) {
	console.log(text);
	if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${text}\n`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	const root = resolve('distribution/npm');
	const manifest = readJson(join(root, 'targets.json'));
	const mode = process.argv[2];
	if (mode === 'matrix') {
		const include = JSON.stringify(runtimeMatrix(manifest));
		console.log(include);
		if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `include=${include}\n`);
	} else if (mode === 'inventory') {
		summary(
			`### Distribution evidence: ${process.env.RELEASE_TAG}\n\nBuilt records producer archive evidence, not a native execution.\n\n`
				+ '| Target | Built | Packaged | Installed | Executed | Policy |\n| --- | --- | --- | --- | --- | --- |\n'
				+ inventory(manifest, root, process.env.RELEASE_TAG),
		);
	} else {
		throw new Error('Expected matrix or inventory');
	}
}
