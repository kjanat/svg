#!/usr/bin/env bun
/// <reference types="bun-types" />

// Create one signed `<package>-v<version>` tag per published package (crates
// and npm alike) pointing at the release tag's commit. Tag names sanitize
// npm scopes the npm-pack way: `@kjanat/svg-toolkit` -> `kjanat-svg-toolkit`.
//
// Candidates come from the release tag's targets.json unioned with the
// current one (facade sets change over time), plus the publishable workspace
// crates; each candidate is then existence-checked against the npm registry
// and the crates.io sparse index so only actually-published packages get a
// tag. Usage:
//
//   bun scripts/release-tags.ts <version> [--dry-run]

import { error, log } from 'node:console';
import { argv, exit } from 'node:process';

const version = argv[2];
const dryRun = argv.includes('--dry-run');
if (!version || !/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
	error('usage: bun scripts/release-tags.ts <version> [--dry-run]');
	exit(1);
}
const releaseTag = `v${version}`;

const tagCheck = await Bun.$`git rev-parse --verify refs/tags/${releaseTag}`.nothrow().quiet();
if (tagCheck.exitCode !== 0) {
	error(`release tag ${releaseTag} does not exist`);
	exit(1);
}
const commit = (await Bun.$`git rev-list -n 1 ${releaseTag}`.text()).trim();

function tagNameFor(packageName: string): string {
	return `${packageName.replace(/^@/, '').replace(/\//g, '-')}-${releaseTag}`;
}

interface TargetsJson {
	scope: string;
	facades: { name: string; shim?: string; alsoPublishAs?: string[]; pkg: string }[];
	bundle?: { name: string };
	grammars?: string[];
	targets: { pkg: string }[];
}

function npmNames(targets: TargetsJson): string[] {
	const names: string[] = [];
	for (const facade of targets.facades) {
		names.push(facade.name, ...(facade.alsoPublishAs ?? []));
		if (facade.shim) names.push(facade.shim);
		for (const target of targets.targets) {
			names.push(`${targets.scope}/${facade.pkg}-${target.pkg}`);
		}
	}
	if (targets.bundle) names.push(targets.bundle.name);
	names.push(...(targets.grammars ?? []));
	return names;
}

async function targetsAt(ref: string): Promise<TargetsJson | null> {
	const shown = await Bun.$`git show ${ref}:distribution/npm/targets.json`.nothrow().quiet();
	if (shown.exitCode !== 0) return null;
	return JSON.parse(shown.stdout.toString()) as TargetsJson;
}

const candidates = new Set<string>();

const atTag = await targetsAt(releaseTag);
const atHead = await targetsAt('HEAD');
for (const targets of [atTag, atHead]) {
	if (!targets) continue;
	for (const name of npmNames(targets)) candidates.add(name);
}

interface CargoMetadata {
	packages: { name: string; publish: string[] | null }[];
	workspace_members: string[];
}

const metadata = JSON.parse(
	await Bun.$`cargo metadata --no-deps --format-version 1`.text(),
) as CargoMetadata;
const crates = metadata.packages
	.filter((pkg) => pkg.publish === null)
	.map((pkg) => pkg.name);
for (const crate of crates) candidates.add(crate);

async function onNpm(name: string): Promise<boolean> {
	const encoded = name.replace('/', '%2F');
	const res = await fetch(`https://registry.npmjs.org/${encoded}/${version}`);
	return res.ok;
}

async function onCratesIo(name: string): Promise<boolean> {
	const prefix = name.length >= 4
		? `${name.slice(0, 2)}/${name.slice(2, 4)}`
		: name.length === 3
		? `3/${name[0]}`
		: `${name.length}`;
	const res = await fetch(`https://index.crates.io/${prefix}/${name}`, {
		headers: { 'user-agent': 'svg release-tags (+https://github.com/kjanat/svg)' },
	});
	if (!res.ok) return false;
	const body = await res.text();
	return body.split('\n').some((line) => {
		if (line.trim() === '') return false;
		const entry = JSON.parse(line) as { vers: string };
		return entry.vers === version;
	});
}

const published: string[] = [];
const skipped: string[] = [];
for (const name of [...candidates].sort()) {
	const exists = (await onNpm(name)) || (await onCratesIo(name));
	if (exists) published.push(name);
	else skipped.push(name);
}

if (skipped.length > 0) {
	log(`not published at ${version} (no tag): ${skipped.join(', ')}`);
}
if (published.length === 0) {
	error(`nothing published at ${version}; refusing to create zero tags`);
	exit(1);
}

const created: string[] = [];
for (const name of published) {
	const tag = tagNameFor(name);
	const existing = await Bun.$`git rev-parse --verify refs/tags/${tag}`.nothrow().quiet();
	if (existing.exitCode === 0) {
		log(`exists: ${tag}`);
		continue;
	}
	if (dryRun) {
		log(`would tag: ${tag} -> ${commit.slice(0, 9)}`);
		continue;
	}
	await Bun.$`git tag -s ${tag} -m ${tag} ${commit}`;
	log(`tagged: ${tag} -> ${commit.slice(0, 9)}`);
	created.push(tag);
}

if (!dryRun && created.length > 0) {
	log(`\n${created.length} tags created; push with:`);
	log(`git push origin ${created.map((t) => `refs/tags/${t}`).join(' ')}`);
}
