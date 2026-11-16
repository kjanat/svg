#!/usr/bin/env bun
import { readFile, writeFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { carriers, workspaceVersion } from '../.github/actions/version-check/carriers.mjs';

// SemVer 2.0.0 grammar (https://semver.org/#is-there-a-suggested-regular-expression-regex-to-check-a-semver-string).
const numeric = String.raw`0|[1-9]\d*`;
const prereleaseId = String.raw`${numeric}|\d*[A-Za-z-][0-9A-Za-z-]*`;
const buildId = String.raw`[0-9A-Za-z-]+`;
const semver = new RegExp(
	String.raw`^(?:${numeric})\.(?:${numeric})\.(?:${numeric})`
		+ String.raw`(?:-(?:${prereleaseId})(?:\.(?:${prereleaseId}))*)?`
		+ String.raw`(?:\+${buildId}(?:\.${buildId})*)?$`,
);

export function isSemanticVersion(version: string): boolean {
	return semver.test(version);
}

export async function versionEdits(version: string): Promise<Map<string, string>> {
	if (!isSemanticVersion(version)) throw new Error('Expected a semantic version');
	const files = new Map<string, string>();
	const cargo = await readFile('Cargo.toml', 'utf8');
	workspaceVersion(cargo);
	files.set('Cargo.toml', cargo.replace(/(\[workspace\.package\][\s\S]*?^version\s*=\s*")[^"]+(".*$)/m, `$1${version}$2`));
	for (const carrier of carriers) {
		let source;
		try {
			source = files.get(carrier.file) ?? await readFile(carrier.file, 'utf8');
		} catch (error) {
			if (error && typeof error === 'object' && 'code' in error && error.code === 'ENOENT') continue;
			throw error;
		}
		let count = 0;
		const updated = source.replace(carrier.pattern, (full: string, prior: string) => {
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
