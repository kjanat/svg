import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { extractBinariesFromTarball } from '../../distribution/npm/scripts/build-packages.ts';

function archive(entries: Record<string, string>): { path: string; cleanup: () => void } {
	const dir = mkdtempSync(join(tmpdir(), 'svg-build-packages-'));
	const source = join(dir, 'source');
	for (const [name, content] of Object.entries(entries)) {
		const file = join(source, name);
		mkdirSync(join(file, '..'), { recursive: true });
		writeFileSync(file, content);
		chmodSync(file, name.replace(/^\.\//, '').includes('/') ? 0o644 : 0o755);
	}
	const path = join(dir, 'release.tar.gz');
	const packed = spawnSync('tar', ['-czf', path, '-C', source, ...Object.keys(entries)], { encoding: 'utf8' });
	expect(packed.stderr).toBe('');
	expect(packed.status).toBe(0);
	return { path, cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

test('root binaries win over same-named shell completions', async () => {
	const { path, cleanup } = archive({
		'svg-lint': 'lint binary',
		'completions/bash/svg-lint': 'lint completion',
		'completions/zsh/_svg-lint': 'lint zsh completion',
	});
	try {
		const found = await extractBinariesFromTarball(path, ['svg-lint']);
		expect([...found.keys()]).toEqual(['svg-lint']);
		expect(found.get('svg-lint')?.toString()).toBe('lint binary');
	} finally {
		cleanup();
	}
});

test('dot-prefixed root binaries and Windows executables are found', async () => {
	const { path, cleanup } = archive({
		'./svg-format': 'format binary',
		'svg-lint.exe': 'lint executable',
		'completions/bash/svg-format': 'format completion',
	});
	try {
		const found = await extractBinariesFromTarball(path, ['svg-format', 'svg-lint']);
		expect(found.get('./svg-format')).toBeUndefined();
		expect(found.get('svg-format')?.toString()).toBe('format binary');
		expect(found.get('svg-lint.exe')?.toString()).toBe('lint executable');
		expect(found.size).toBe(2);
	} finally {
		cleanup();
	}
});

test('a binary only present below the archive root is missing', async () => {
	const { path, cleanup } = archive({ 'nested/svg-lint': 'nested binary' });
	try {
		expect((await extractBinariesFromTarball(path, ['svg-lint'])).size).toBe(0);
	} finally {
		cleanup();
	}
});
