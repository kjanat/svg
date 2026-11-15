#!/usr/bin/env node
// @ts-check
import { readFile, writeFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { carriers, workspaceVersion } from '../.github/actions/version-check/carriers.mjs';

/** @param {string} version @param {typeof readFile} [read] */
export async function versionEdits(version, read = readFile) {
	if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) throw new Error('Expected a semantic version');
	const files = new Map();
	const cargo = await read('Cargo.toml', 'utf8');
	workspaceVersion(cargo);
	files.set('Cargo.toml', cargo.replace(/(\[workspace\.package\][\s\S]*?^version\s*=\s*")[^"]+(".*$)/m, `$1${version}$2`));
	for (const carrier of carriers) {
		let source;
		try {
			source = files.get(carrier.file) ?? await read(carrier.file, 'utf8');
		} catch (error) {
			if (error && typeof error === 'object' && 'code' in error && error.code === 'ENOENT') continue;
			throw error;
		}
		let count = 0;
		const updated = source.replace(carrier.pattern, (full, prior) => {
			count++;
			const start = full.lastIndexOf(prior);
			return full.slice(0, start) + version + full.slice(start + prior.length);
		});
		if (!count) throw new Error(`Version pattern missing: ${carrier.file}`);
		files.set(carrier.file, updated);
	}
	return files;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
	const edits = await versionEdits(process.argv[2] ?? '');
	for (const [path, content] of edits) await writeFile(path, content);
}
