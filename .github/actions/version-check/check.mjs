#!/usr/bin/env node
// @ts-check
// Verify that every version carrier matches the workspace version in
// Cargo.toml. Exits non-zero listing each mismatch. Runs from the repo root.

import { readFile } from 'node:fs/promises';
import { exit } from 'node:process';
import { carriers, workspaceVersion } from './carriers.mjs';

const version = workspaceVersion(await readFile('Cargo.toml', 'utf8'));

/** @type {string[]} */
const mismatches = [];
for (const carrier of carriers) {
	/** @type {string} */
	let source;
	try {
		source = await readFile(carrier.file, 'utf8');
	} catch {
		continue;
	}
	const match = source.match(carrier.pattern);
	if (!match) {
		mismatches.push(`${carrier.file}: version pattern not found`);
		continue;
	}
	if (match[1] !== version) {
		mismatches.push(`${carrier.file}: ${match[1]} (workspace is ${version})`);
	}
}

if (mismatches.length > 0) {
	console.error(`version carriers out of sync with workspace ${version}:`);
	for (const mismatch of mismatches) console.error(`  ${mismatch}`);
	exit(1);
}
console.log(`all version carriers match workspace ${version}`);
