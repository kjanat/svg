import assert from 'node:assert/strict';
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import {
	archivePath,
	assertExecutable,
	assertVersion,
	cmdLine,
	digest,
	expectedVersion,
	inventory,
	readJson,
	run,
	runCommand,
	runtimeMatrix,
	targetFor,
	verifyChecksum,
} from '../actions/npm-smoke/runtime.mjs';

const manifest = readJson(new URL('../../distribution/npm/targets.json', import.meta.url));

test('all eight tier-1 runtime environments come from the single manifest', () => {
	const matrix = runtimeMatrix(manifest);
	assert.equal(matrix.length, 8);
	assert.deepEqual(new Set(matrix.map(t => t.pkg)), new Set(manifest.targets.filter(t => t.tier === 1).map(t => t.pkg)));
	for (const target of manifest.targets.filter(t => t.runtime)) {
		const entry = matrix.find(t => t.pkg === target.pkg);
		assert.equal(entry.target, target.rust);
		assert.equal(entry.arch, target.cpu[0]);
		assert.equal(entry.runner, target.runtime.runner);
		if (target.libc?.includes('musl')) assert.match(entry.container, /-alpine$/);
	}
});

test('empty, duplicated, ambiguous, experimental or fake-musl runtime plans fail', () => {
	const change = fn => {
		const copy = structuredClone(manifest);
		fn(copy);
		return copy;
	};
	assert.throws(() => runtimeMatrix(change(m => m.targets.forEach(t => delete t.runtime))), /No required runtime/);
	assert.throws(() => runtimeMatrix(change(m => m.targets.push(m.targets[0]))), /Duplicate/);
	assert.throws(() => runtimeMatrix(change(m => m.targets[0].cpu.push('arm64'))), /unambiguous/);
	assert.throws(() => runtimeMatrix(change(m => m.targets[0].experimental = true)), /build-only/);
	assert.throws(() => runtimeMatrix(change(m => delete m.targets.find(t => t.pkg === 'linux-x64-musl').runtime.container)), /musl/);
	assert.throws(() => targetFor(manifest, 'freebsd-x64'), /No runtime configured/);
});

test('exact version tokens reject substring matches and malformed release inputs', () => {
	assert.equal(expectedVersion('v1.2.3-rc.4'), '1.2.3-rc.4');
	assertVersion('svg-lint 1.2.3\n', '1.2.3', 'lint');
	for (const output of ['svg-lint 1.2.30', 'svg-lint 11.2.3', 'svg-lint 1.2.3-old', '']) {
		assert.throws(() => assertVersion(output, '1.2.3', 'lint'), /expected exact version/);
	}
	assert.throws(() => expectedVersion('v1.2.3\ninclude=oops'), /Invalid release tag/);
});

test('Windows shim invocation preserves spaces and disables command expansion', () => {
	assert.equal(cmdLine('C:\\temp path\\svg-ls.cmd', ['--version']), '""C:\\temp path\\svg-ls.cmd" "--version""');
	for (const arg of ['%PATH%', 'a"b', 'line\nbreak']) assert.throws(() => cmdLine('npm', [arg]), /Unsupported cmd/);
});

test('subprocess callers cannot enable shell interpretation', () => {
	assert.equal(
		run(process.execPath, ['-e', 'process.stdout.write(process.argv[1])', 'one argument with spaces'], { shell: true }),
		'one argument with spaces',
	);
});

test('Windows executes an absolute batch shim from a different working directory', t => {
	if (process.platform !== 'win32') return t.skip('Windows-only .cmd behavior');
	const root = mkdtempSync(join(tmpdir(), 'svg shim test '));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const shim = join(root, 'entry.cmd');
	writeFileSync(shim, '@echo off\r\necho %~dp0\r\n');
	assert.equal(runCommand(shim, [], { cwd: tmpdir() }), `${root}\\`);
});

test('an incompatible host fails without reporting installation or execution success', t => {
	const root = mkdtempSync(join(tmpdir(), 'svg-host-test-'));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const host = process.platform === 'win32' ? 'linux-x64-gnu' : 'win32-x64-msvc';
	const reportDir = join(root, 'reports');
	assert.throws(() =>
		run(process.execPath, [fileURLToPath(new URL('../actions/npm-smoke/run.mjs', import.meta.url))], {
			env: { ...process.env, RELEASE_TAG: 'v1.2.3', HOST_PKG: host, RUNTIME_RESULTS: reportDir },
		}), /Runtime OS does not match/);
	const report = readJson(join(reportDir, `${host}.json`));
	assert.equal(report.built, 'FAILED');
	assert.equal(report.installed, 'not completed');
	assert.equal(report.executed, 'not completed');
});

test('Unix executable bits are required before npm can repair them', t => {
	if (process.platform === 'win32') return t.skip('Windows records execution through .exe and .cmd smoke tests');
	const root = mkdtempSync(join(tmpdir(), 'svg-mode-test-'));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const file = join(root, 'binary');
	writeFileSync(file, '#!/bin/sh\n', { mode: 0o644 });
	assert.throws(() => assertExecutable(file), /permission lost/);
	chmodSync(file, 0o755);
	assertExecutable(file);
});

test('archive identity rejects changed bytes, wrong checksum names and missing checksums', t => {
	const root = mkdtempSync(join(tmpdir(), 'svg-checksum-test-'));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const archive = join(root, 'archive.tar.gz');
	const checksum = join(root, 'archive.sha256');
	writeFileSync(archive, 'original');
	assert.throws(() => verifyChecksum(archive), /ENOENT/);
	writeFileSync(checksum, `${digest(archive)}  archive.tar.gz\n`);
	verifyChecksum(archive);
	writeFileSync(archive, 'changed');
	assert.throws(() => verifyChecksum(archive), /checksum mismatch/);
	writeFileSync(checksum, `${digest(archive)}  other.tar.gz\n`);
	assert.throws(() => verifyChecksum(archive), /different archive/);
});

test('inventory never turns build-only or absent targets into runtime success', t => {
	const root = mkdtempSync(join(tmpdir(), 'svg-inventory-test-'));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const target = { ...manifest.targets[0] };
	delete target.runtime;
	const small = { ...manifest, targets: [target, { ...target, pkg: 'experimental', rust: 'optional-unknown-linux-gnu', experimental: true }] };
	mkdirSync(join(root, 'downloads'));
	const archive = archivePath(join(root, 'downloads'), 'v1.2.3', target);
	writeFileSync(archive, 'producer archive fixture');
	writeFileSync(archive.replace(/\.tar\.gz$/, '.sha256'), `${digest(archive)}  ${basename(archive)}\n`);
	assert.throws(() => inventory(small, root, 'v1.2.3'), /Required target not built\/packaged/);
	for (const facade of small.facades) {
		const path = join(root, 'dist', `${facade.pkg}-${target.pkg}`);
		mkdirSync(join(path, 'bin'), { recursive: true });
		writeFileSync(join(path, 'package.json'), JSON.stringify({ name: `${small.scope}/${facade.pkg}-${target.pkg}`, version: '1.2.3' }));
		writeFileSync(join(path, 'bin', facade.bin), 'fixture binary');
	}
	const report = inventory(small, root, 'v1.2.3');
	assert.match(report, /archive verified \| present \| not run \| not run \| build only/);
	assert.match(report, /experimental \| absent \| absent \| not run \| not run \| experimental, non-blocking build only/);
});
