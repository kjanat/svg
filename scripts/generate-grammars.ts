#!/usr/bin/env bun
/// <reference types="bun" />
import { execFileSync } from 'node:child_process';
import { copyFile, mkdir, mkdtemp, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const root = resolve(import.meta.dir, '..');
const check = process.argv.includes('--check');
if (process.argv.slice(2).some(arg => arg !== '--check')) throw new Error('Usage: bun scripts/generate-grammars.ts [--check]');
const grammars = ['svg', 'svg-path', 'svg-paint', 'svg-transform'];
const host = join(root, 'grammars/tree-sitter-svg');
// Resolve the installed workspace dependency, never a global CLI or an unpinned download.
const require = createRequire(join(host, 'package.json'));
const cli = require.resolve('tree-sitter-cli/cli.js');
const catalog = await readFile(join(root, 'crates/svg-data/data/catalog.tree-sitter.json'), 'utf8');
const { tokens } = JSON.parse(catalog) as { tokens: Record<string, string[]> };

async function updateConstants(path: string, entries: Record<string, string>): Promise<void> {
	let source = await readFile(path, 'utf8');
	const existing = await import(pathToFileURL(path).href);
	for (const [name, key] of Object.entries(entries)) {
		const values = tokens[key];
		if (!Array.isArray(values) || values.some(value => typeof value !== 'string')) throw new Error(`Invalid catalog tokens.${key}`);
		// Compare values so existing comments, grouping and whitespace remain intact.
		if (JSON.stringify(existing[name]) === JSON.stringify(values)) continue;
		if (check) throw new Error(`${relative(root, path)}: ${name} is stale; run just generate-grammars`);
		const declaration = new RegExp(`(export const ${name} = (?:/\\* dprint-ignore \\*/ )?\\[)([\\s\\S]*?)(\\];)`);
		if (!declaration.test(source)) throw new Error(`Cannot locate ${name} in ${relative(root, path)}`);
		const literals = values.map(value =>
			`'${value.replaceAll('\\', '\\\\').replaceAll("'", "\\'").replaceAll('\n', '\\n').replaceAll('\r', '\\r')}'`
		);
		source = source.replace(declaration, (_, start: string, body: string, end: string) => {
			const indent = body.match(/\n([\t ]+)\S/)?.[1] ?? '\t';
			const replacement = body.includes('\n') ? `\n${literals.map(value => `${indent}${value},`).join('\n')}\n` : literals.join(', ');
			return start + replacement + end;
		});
	}
	await update(path, source);
}

async function current(path: string, expected: Uint8Array): Promise<boolean> {
	try {
		return Buffer.from(expected).equals(await readFile(path));
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false;
		throw error;
	}
}

async function update(path: string, content: string): Promise<void> {
	if (await current(path, Buffer.from(content))) return;
	if (check) throw new Error(`${relative(root, path)} is stale; run just generate-grammars`);
	await writeFile(path, content);
	console.log(`Updated ${relative(root, path)}`);
}

execFileSync('bun', ['run', 'codegen', ...(check ? ['--check'] : [])], { cwd: host, stdio: 'inherit' });
await updateConstants(
	join(root, 'grammars/tree-sitter-svg-path/constants.js'),
	{ PATH_COMMAND_LETTERS: 'path_command_letters' },
);
await updateConstants(
	join(root, 'grammars/tree-sitter-svg-paint/constants.js'),
	{
		COLOR_SPACES: 'color_spaces',
		COLOR_INTERPOLATION_SPACES: 'color_interpolation_spaces',
		HUE_INTERPOLATION_METHODS: 'hue_interpolation_methods',
		ANGLE_UNITS: 'angle_units',
	},
);

// Generate into a scratch output directory so --check never rewrites the checkout.
// Only generator outputs are copied; hand-written scanners remain untouched.
const scratch = await mkdtemp(join(tmpdir(), 'svg-grammar-generation-'));
try {
	const stale: string[] = [];
	for (const grammar of grammars) {
		const cwd = join(root, 'grammars', `tree-sitter-${grammar}`);
		const output = join(scratch, grammar);
		execFileSync('node', [cli, 'generate', '--abi', '15', '--output', output], { cwd, stdio: 'inherit' });
		for (const file of await readdir(output, { recursive: true, withFileTypes: true })) {
			if (!file.isFile()) continue;
			const source = join(file.parentPath, file.name);
			const target = join(cwd, 'src', relative(output, source));
			if (await current(target, await readFile(source))) continue;
			if (check) {
				stale.push(relative(root, target));
			} else {
				await mkdir(dirname(target), { recursive: true });
				await copyFile(source, target);
				console.log(`Updated ${relative(root, target)}`);
			}
		}
	}
	if (stale.length) throw new Error(`Generated grammar artifacts are stale; run just generate-grammars:\n${stale.join('\n')}`);
	execFileSync('bun', ['run', check ? 'gen:corpus:check' : 'gen:corpus'], { cwd: host, stdio: 'inherit' });
	console.log(check ? 'All grammar artifacts are current.' : 'All grammar artifacts generated.');
} finally {
	await rm(scratch, { recursive: true, force: true });
}
