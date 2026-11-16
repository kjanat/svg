import { expect, test } from 'bun:test';
import { isSemanticVersion, versionEdits } from '../../scripts/release-version.ts';
import { carriers, workspaceVersion } from '../actions/version-check/carriers.mjs';

test('workflow preparation updates every version carrier without tags or commits', async () => {
	const edits = await versionEdits('0.5.0');
	const cargo = edits.get('Cargo.toml');
	if (cargo === undefined) throw new Error('Workspace version update is missing');
	expect(workspaceVersion(cargo)).toBe('0.5.0');
	for (const carrier of carriers) {
		const content = edits.get(carrier.file);
		if (content === undefined) continue;
		const matches = carrier.all ? [...content.matchAll(carrier.pattern)] : [content.match(carrier.pattern)];
		expect(matches.length).toBeGreaterThan(0);
		for (const match of matches) expect(match?.[1]).toBe('0.5.0');
	}
	await expect(versionEdits('0.5.0\nmalformed')).rejects.toThrow('Expected a semantic version');
});

test('semantic version guard follows SemVer 2.0.0', () => {
	for (const valid of [
		'0.0.0',
		'1.2.3',
		'10.20.30',
		'1.2.3-rc.1',
		'1.2.3-0',
		'1.2.3-alpha.0.beta',
		'1.2.3-x-y.7z',
		'1.2.3+build.5',
		'1.2.3-rc.1+build.5',
		'1.2.3+001',
		'1.2.3-0a',
	]) {
		expect(isSemanticVersion(valid)).toBe(true);
	}
	for (const invalid of [
		'',
		'1.2',
		'1.2.3.4',
		'v1.2.3',
		'01.2.3',
		'1.02.3',
		'1.2.03',
		'1.2.3-',
		'1.2.3-..',
		'1.2.3-rc..1',
		'1.2.3-01',
		'1.2.3-rc.01',
		'1.2.3+',
		'1.2.3+a..b',
		'1.2.3-rc_1',
		'1.2.3\n',
		' 1.2.3',
	]) {
		expect(isSemanticVersion(invalid)).toBe(false);
	}
});

test('version preparation rejects invalid versions before computing edits', async () => {
	for (const invalid of ['01.2.3', '1.2.3-..', '1.2.3-01']) await expect(versionEdits(invalid)).rejects.toThrow('Expected a semantic version');
	const version = '1.2.3-rc.1+build.5';
	const edits = await versionEdits(version);
	const cargo = edits.get('Cargo.toml');
	if (cargo === undefined) throw new Error('Workspace version update is missing');
	expect(workspaceVersion(cargo)).toBe(version);
	for (const carrier of carriers) {
		const content = edits.get(carrier.file);
		if (content === undefined) continue;
		const matches = carrier.all ? [...content.matchAll(carrier.pattern)] : [content.match(carrier.pattern)];
		expect(matches.length).toBeGreaterThan(0);
		for (const match of matches) expect(match?.[1]).toBe(version);
	}
});
