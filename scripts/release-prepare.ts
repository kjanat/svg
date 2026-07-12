#!/usr/bin/env bun
/// <reference types="bun-types" />

import { error, log } from 'node:console';
import { normalize, relative } from 'node:path';
import { argv, cwd, exit } from 'node:process';
import { carriers } from '../.github/actions/version-check/carriers.mjs';

const version = argv[2];
if (!version || !/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
	error(`usage: ${relative(cwd(), import.meta.path)} <version>`);
	exit(1);
}

const status = (await Bun.$`git status --short`.text()).trim();
if (status !== '') {
	error('working tree must be clean before preparing a release');
	exit(1);
}

const tag = `v${version}`;
const tagRef = `refs/tags/${tag}`;
const tagCheck = await Bun.$`git rev-parse --verify ${tagRef}`.nothrow().quiet();
if (tagCheck.exitCode === 0) {
	error(`tag ${tag} already exists`);
	exit(1);
}

const cargoTomlPath = normalize(`${import.meta.dir}/../Cargo.toml`);
const cargoTomlSource = await Bun.file(cargoTomlPath).text();
const cargoTomlParsed = Bun.TOML.parse(cargoTomlSource) as {
	workspace?: { package?: { version?: string } };
};

if (cargoTomlParsed.workspace?.package?.version == null) {
	error('Cargo.toml is missing workspace.package.version');
	exit(1);
}

const cargoToml = cargoTomlSource
	.replace(
		/(\[workspace\.package\][\s\S]*?version\s*=\s*")(.*?)("\n)/,
		`$1${version}$3`,
	)
	// Internal workspace deps carry crates.io version requirements that must
	// track the workspace version, or `cargo publish --workspace` breaks the
	// release after the first bump.
	.replace(
		/(= \{ package = "(?:svg-|tree-sitter-svg)[\w-]*", version = ")([^"]+)(")/g,
		`$1${version}$3`,
	);

if (cargoToml === cargoTomlSource) {
	error('failed to update workspace.package.version in Cargo.toml');
	exit(1);
}

const internalDeps = Array.from(
	cargoToml.matchAll(/= \{ package = "(?:svg-|tree-sitter-svg)[\w-]*", version = "([^"]+)"/g),
);
const bumpedDepCount = internalDeps.filter(([, depVersion]) => depVersion === version).length;
if (internalDeps.length !== bumpedDepCount) {
	error(`only ${bumpedDepCount}/${internalDeps.length} internal dep version requirements were bumped to ${version}`);
	exit(1);
}

const updatedCargoToml = Bun.TOML.parse(cargoToml) as {
	workspace?: { package?: { version?: string } };
};

if (updatedCargoToml.workspace?.package?.version !== version) {
	error('Cargo.toml version update did not produce the expected workspace.package.version');
	exit(1);
}

await Bun.write(cargoTomlPath, cargoToml);

// Every non-Cargo version carrier (grammar manifests, zed extension, root
// tree-sitter.json) tracks the workspace version; the shared list also backs
// version-check.ts, so a carrier this misses fails CI instead of shipping
// stale.
const repoRoot = normalize(`${import.meta.dir}/..`);
for (const carrier of carriers) {
	const path = `${repoRoot}/${carrier.file}`;
	const file = Bun.file(path);
	if (!(await file.exists())) continue;
	const source = await file.text();
	const match = source.match(carrier.pattern);
	if (!match || match.index == null) {
		error(`version pattern not found in ${carrier.file}`);
		exit(1);
	}
	const versionStart = match.index + match[0].indexOf(match[1]);
	const updated = source.slice(0, versionStart) + version + source.slice(versionStart + match[1].length);
	await Bun.write(path, updated);
	log(`bumped: ${carrier.file}`);
}

await Bun.$`cargo check --workspace`;
await Bun.$`just verify`;
await Bun.$`git add Cargo.toml Cargo.lock grammars editors/zed-svg/extension.toml tree-sitter.json`;
await Bun.$`git commit -m ${`chore(release): ${tag}`}`;
await Bun.$`git tag -s ${tag} -m ${tag}`;

const branch = (await Bun.$`git branch --show-current`.text()).trim();
log('release prepared locally');
log(`next: git push origin ${branch}`);
log(`next: git push origin ${tag}`);
