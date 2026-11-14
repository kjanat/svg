#!/usr/bin/env node
/**
 * Catalog-seeded corpus generator.
 *
 * Emits a corpus case for every element/attribute/sample combination, using
 * element-specific value definitions and retaining distinct value forms. The
 * expected syntax tree for every case is CAPTURED from the built grammar
 * (`tree-sitter parse --no-ranges`) rather than hand-authored, so the generated
 * file always reflects what the parser really produces. Missing samples and
 * parse errors fail generation instead of silently dropping coverage.
 *
 * Sources of truth:
 *   - crates/svg-data/data/catalog.core.json        (elements, attrs, enums)
 *
 * Output (deterministic, regenerable):
 *   - test/corpus/generated_catalog_elements.txt
 *
 * Modes:
 *   (default)  write the corpus file
 *   --check    regenerate in memory and fail if the on-disk file is stale
 */

import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { argv, execPath } from 'node:process';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const grammarRoot = resolve(scriptDir, '..');
const repoRoot = resolve(grammarRoot, '../..');
const corePath = resolve(repoRoot, 'crates/svg-data/data/catalog.core.json');
const outPath = resolve(grammarRoot, 'test/corpus/generated_catalog_elements.txt');
const cli = createRequire(import.meta.url).resolve('tree-sitter-cli/cli.js');

const GENERATED_HEADER = [
	'; GENERATED FILE - DO NOT EDIT BY HAND.',
	'; Source: scripts/gen-corpus.ts (catalog-seeded corpus generator).',
	'; Regenerate: `bun run gen:corpus`  Verify: `bun run gen:corpus:check`.',
	'; Expected trees are captured from the built grammar, not hand-authored.',
].join('\n');

const DIVIDER = '='.repeat(40);

/** A keyword bucket attribute's value space, taken from the core catalog. */
type AttributeValues =
	| { readonly kind: 'enum'; readonly values: readonly string[] }
	| { readonly kind: 'transform'; readonly functions: readonly string[] }
	| { readonly kind: 'color' }
	| { readonly kind: 'length' }
	| { readonly kind: 'url' }
	| { readonly kind: 'boolean' }
	| { readonly kind: 'token_list' }
	| { readonly kind: 'comma_token_list' }
	| { readonly kind: 'url_token_list' }
	| { readonly kind: 'language_tag' }
	| { readonly kind: 'integer' }
	| { readonly kind: 'number' }
	| { readonly kind: 'id_list' }
	| { readonly kind: 'paint' }
	| { readonly kind: 'unresolved' }
	| { readonly kind: 'media_type' }
	| { readonly kind: 'media_query_list' }
	| { readonly kind: 'css_declaration_list' }
	| { readonly kind: 'id' }
	| { readonly kind: 'referrer_policy' }
	| { readonly kind: 'suggested_file_name' }
	| { readonly kind: 'path_data' }
	| { readonly kind: 'semicolon_number_list' }
	| { readonly kind: 'coordinate_pair' }
	| { readonly kind: 'coordinate_pair_list' }
	| { readonly kind: 'number_or_percentage' }
	| { readonly kind: 'free_text' }
	| { readonly kind: 'css_grammar'; readonly grammar: string; readonly graph: CssGrammarGraph };

type CssGrammarGraph = {
	readonly nodes?: readonly { readonly kind?: string; readonly text?: string }[];
};

type ElementValues = {
	readonly element: string;
	readonly values: AttributeValues;
};

type CoreAttribute = {
	readonly name: string;
	readonly values?: AttributeValues;
	readonly element_values?: readonly ElementValues[];
};

type CoreElement = {
	readonly name: string;
	readonly attrs?: readonly string[];
};

type CoreCatalog = {
	readonly elements: readonly CoreElement[];
	readonly attributes: readonly CoreAttribute[];
};

function displayPath(path: string): string {
	const rel = relative(repoRoot, path);
	return rel === '' || rel.startsWith('..') ? path : rel;
}

function readJson<T>(path: string): T {
	return JSON.parse(readFileSync(path, 'utf8')) as T;
}

function valuesForElement(attr: CoreAttribute | undefined, elementName: string): AttributeValues | undefined {
	return attr?.element_values?.find((elementValues) => elementValues.element === elementName)?.values
		?? attr?.values;
}

function firstKeyword(values: AttributeValues): string | null {
	if (values.kind === 'enum' && values.values.length > 0) {
		return [...values.values].sort()[0] ?? null;
	}
	if (values.kind !== 'css_grammar') {
		return null;
	}
	const keywords = (values.graph.nodes ?? [])
		.filter((node) => node.kind === 'keyword' && node.text !== undefined)
		.map((node) => node.text ?? '')
		.sort();
	return keywords[0] ?? null;
}

export function cssGrammarSample(values: Extract<AttributeValues, { readonly kind: 'css_grammar' }>): string | null {
	return cssGrammarSamples(values)[0] ?? null;
}

/** Representative forms for the value grammars used by element attributes. */
function cssGrammarSamples(values: Extract<AttributeValues, { readonly kind: 'css_grammar' }>): string[] {
	const grammar = values.grammar.toLowerCase();
	if (grammar === '[<min-x>,? <min-y>,? <width>,? <height>]') return ['0 0 10 10', '0,0,10,10'];
	if (grammar === '<align> <meetorslice>?') return ['none', 'xMidYMid', 'xMidYMid meet', 'xMidYMid slice'];
	const typeSamples: Readonly<Record<string, readonly string[]>> = {
		'<number>': ['0.5'],
		'<integer>': ['1'],
		'<length>': ['5', '5px'],
		'<length-percentage>': ['5', '5px', '50%'],
		'<angle>': ['5deg'],
		'<number-optional-number>': ['2 3', '2'],
		'<points>': ['0,0', '0,0 1,1'],
		'<blend-mode>': [
			'normal',
			'multiply',
			'screen',
			'overlay',
			'darken',
			'lighten',
			'color-dodge',
			'color-burn',
			'hard-light',
			'soft-light',
			'difference',
			'exclusion',
			'hue',
			'saturation',
			'color',
			'luminosity',
		],
		'<filter-primitive-reference>': ['source'],
		'<xml-name>': ['target'],
		'<string>': ['M0 0 L1 1'],
		'<value>': ['0.5'],
		'<url>': ['url(#f)'],
		'<iri>': ['url(#f)'],
		'<color>': ['red'],
		'<paint>': ['red', 'none', 'url(#f)'],
	};
	// Multipliers apply to the preceding type or bracketed group, not to every
	// alternative in the grammar (e.g. <number>+ | <angle>).
	const groups: { start: number; end: number }[] = [];
	const openings: number[] = [];
	for (const token of grammar.matchAll(/<[^>]+>|\[|\]/g)) {
		if (token[0] === '[') openings.push(token.index);
		if (token[0] === ']') {
			const start = openings.pop();
			if (start === undefined) throw new Error(`Unbalanced CSS grammar: ${values.grammar}`);
			groups.push({ start, end: token.index });
		}
	}
	if (openings.length) throw new Error(`Unbalanced CSS grammar: ${values.grammar}`);
	const multiplierAfter = (end: number): string | undefined => grammar.slice(end).match(/^\s*([+#?])/)?.[1];
	const samples: string[] = [];
	for (const token of grammar.matchAll(/<[^>]+>/g)) {
		const type = token[0];
		const forms = typeSamples[type];
		if (forms === undefined) throw new Error(`No samples for ${type} in CSS grammar: ${values.grammar}`);
		const multipliers = new Set([
			multiplierAfter(token.index + type.length),
			...groups.filter(group => group.start < token.index && token.index < group.end).map(group => multiplierAfter(group.end + 1)),
		]);
		samples.push(...forms);
		if (multipliers.has('+')) samples.push(type === '<number>' ? '0 1 0' : forms.map(value => `${value} ${value}`).join(' '));
		if (multipliers.has('#')) samples.push(type === '<number>' ? '0, 1, 0' : forms.map(value => `${value}, ${value}`).join(', '));
	}
	for (const node of values.graph.nodes ?? []) {
		if (node.kind === 'keyword' && node.text !== undefined) samples.push(node.text);
	}
	return [...new Set(samples)];
}

function semanticSample(values: AttributeValues | undefined): string | null {
	if (values === undefined) {
		return null;
	}
	switch (values.kind) {
		case 'enum':
			return firstKeyword(values);
		case 'transform':
			return 'translate(1 2)';
		case 'color':
		case 'paint':
			return 'red';
		case 'length':
			return '5';
		case 'url':
			return '#target';
		case 'boolean':
			return 'true';
		case 'token_list':
			return 'alpha beta';
		case 'comma_token_list':
			return 'alpha,beta';
		case 'url_token_list':
			return '#a #b';
		case 'language_tag':
			return 'en';
		case 'integer':
			return '1';
		case 'number':
			return '0.5';
		case 'id_list':
			return 'first second';
		case 'media_type':
			return 'text/css';
		case 'media_query_list':
			return 'screen';
		case 'css_declaration_list':
			return 'fill: red';
		case 'id':
			return 'sample-id';
		case 'referrer_policy':
			return 'no-referrer';
		case 'suggested_file_name':
			return 'example.svg';
		case 'path_data':
			return 'M0 0 L1 1';
		case 'semicolon_number_list':
			return '0; 0.5; 1';
		case 'coordinate_pair':
			return '0,0';
		case 'coordinate_pair_list':
			return '0,0; 1,1';
		case 'number_or_percentage':
			return '0.5';
		case 'css_grammar':
			return cssGrammarSample(values);
		case 'free_text':
			return 'text';
		case 'unresolved':
			return null;
		default: {
			const unsupported: never = values;
			throw new Error(`Unsupported catalog values: ${JSON.stringify(unsupported)}`);
		}
	}
}

function attributeSemanticSample(
	attributeName: string,
	elementName: string,
	values: AttributeValues | undefined,
): string | null {
	if (attributeName === 'type' && values?.kind === 'media_type') {
		if (elementName === 'script') {
			return 'application/ecmascript';
		}
		if (elementName === 'style') {
			return 'text/css';
		}
		if (elementName === 'a') {
			return 'text/html';
		}
	}
	return semanticSample(values);
}

export function attributeSamples(attribute: CoreAttribute, elementName: string): string[] {
	const values = valuesForElement(attribute, elementName);
	let samples: string[];
	if (values?.kind === 'enum') {
		samples = [...values.values].sort();
	} else if (values?.kind === 'css_grammar') {
		samples = cssGrammarSamples(values);
	} else if (values?.kind === 'boolean') {
		samples = ['false', 'true'];
	} else if (values?.kind === 'number_or_percentage') {
		samples = ['0.5', '50%'];
	} else {
		const sample = attributeSemanticSample(attribute.name, elementName, values);
		samples = sample === null ? [] : [sample];
	}
	if (samples.length === 0) throw new Error(`No samples for ${elementName}.${attribute.name} (${values?.kind ?? 'missing values'})`);
	return [...new Set(samples)];
}

function attrString(attrs: readonly { name: string; value: string }[]): string {
	return attrs.map(({ name, value }) => ` ${name}="${value}"`).join('');
}

/**
 * Wrap an element snippet so it is structurally valid. The `svg` element is the
 * document root and carries its own attributes; every other element is a
 * self-closing child of a bare `<svg>` root.
 */
function buildSnippet(
	element: CoreElement,
	attrs: readonly { name: string; value: string }[],
): string {
	if (element.name === 'svg') {
		return `<svg${attrString(attrs)}/>`;
	}
	return `<svg><${element.name}${attrString(attrs)}/></svg>`;
}

type ParseResult = { successful: boolean; sexp: string };

/**
 * Parse a snippet with the built grammar and capture the range-free s-expression
 * exactly as `tree-sitter test` would compare it. `successful` reflects the
 * parser's own ERROR/MISSING verdict via the JSON summary; the s-expression is
 * additionally scanned for recovery markers as a defence in depth.
 */
function parseSnippet(snippet: string, workdir: string): ParseResult {
	const file = join(workdir, 'snippet.svg');
	// `tree-sitter test` feeds the corpus input region with its trailing newline,
	// so capture against the identical bytes to keep expected/actual aligned.
	// With `extras: () => []` that trailing `\n` surfaces as a final `(text)`.
	writeFileSync(file, `${snippet}\n`);

	const summaryResult = spawnSync(
		execPath,
		[cli, 'parse', '--config-path', join(workdir, 'config.json'), '--scope', 'source.svg', '--json-summary', '--quiet', file],
		{ cwd: grammarRoot, encoding: 'utf8' },
	);
	if (summaryResult.error) throw summaryResult.error;
	const summaryRaw = summaryResult.stdout;
	const jsonStart = summaryRaw.indexOf('{');
	if (jsonStart === -1) {
		throw new Error(
			`tree-sitter parse did not emit JSON summary for: ${snippet}\n${summaryRaw}${summaryResult.stderr}`,
		);
	}
	const summary = JSON.parse(summaryRaw.slice(jsonStart)) as {
		parse_summaries: { successful: boolean }[];
	};
	const summarySuccessful = summaryResult.status === 0 && summary.parse_summaries.length === 1
		&& summary.parse_summaries.every((s) => s.successful);

	const sexpResult = spawnSync(
		execPath,
		[cli, 'parse', '--config-path', join(workdir, 'config.json'), '--scope', 'source.svg', '--no-ranges', file],
		{ cwd: grammarRoot, encoding: 'utf8' },
	);
	if (sexpResult.error) throw sexpResult.error;
	const sexp = sexpResult.stdout.trimEnd();

	const clean = summarySuccessful && sexpResult.status === 0 && sexp.length > 0 && !/\b(ERROR|MISSING)\b/.test(sexp)
		&& !sexp.includes('erroneous');
	return { successful: clean, sexp };
}

type GeneratedCase = { title: string; input: string; sexp: string };

export function elementSnippets(
	element: CoreElement,
	coreByName: Map<string, CoreAttribute>,
): Omit<GeneratedCase, 'sexp'>[] {
	const attributes = [...(element.attrs ?? [])].sort().map(name => {
		const attribute = coreByName.get(name);
		if (attribute === undefined) throw new Error(`Missing catalog attribute: ${element.name}.${name}`);
		return { name, samples: attributeSamples(attribute, element.name) };
	});
	const primary = attributes.map(({ name, samples }) => ({ name, value: samples[0]! }));
	const cases = [{
		title: `Element ${element.name}${primary.length ? ' (typed attributes)' : ''}`,
		input: buildSnippet(element, primary),
	}];
	// Keep the combined case and test each attribute/value independently as well.
	const seen = new Set(cases.map(testCase => testCase.input));
	for (const { name, samples } of attributes) {
		for (const value of samples) {
			const input = buildSnippet(element, [{ name, value }]);
			if (seen.has(input)) continue;
			seen.add(input);
			cases.push({ title: `Element ${element.name} (${name}="${value}")`, input });
		}
	}
	return cases;
}

function generateCases(element: CoreElement, coreByName: Map<string, CoreAttribute>, workdir: string): GeneratedCase[] {
	return elementSnippets(element, coreByName).map(testCase => {
		const parsed = parseSnippet(testCase.input, workdir);
		if (!parsed.successful) throw new Error(`${testCase.title} does not parse cleanly: ${testCase.input}\n${parsed.sexp}`);
		return { ...testCase, sexp: parsed.sexp };
	});
}

function renderCorpus(cases: readonly GeneratedCase[]): string {
	// Input follows the divider with no intervening blank line: a leading blank
	// would be fed to the parser as a leading `(text)` node (extras are empty).
	const blocks = cases.map(({ title, input, sexp }) => [DIVIDER, title, DIVIDER, input, '', '---', '', sexp].join('\n'));
	return `${GENERATED_HEADER}\n\n${blocks.join('\n\n')}\n`;
}

function main(): void {
	const checkMode = argv.includes('--check');

	const core = readJson<CoreCatalog>(corePath);
	const coreByName = new Map(core.attributes.map((a) => [a.name, a]));

	const elements = [...core.elements].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));

	const workdir = mkdtempSync(join(tmpdir(), 'svg-gen-corpus-'));
	try {
		writeFileSync(join(workdir, 'config.json'), JSON.stringify({ 'parser-directories': [dirname(grammarRoot)] }));
		const cases = elements.flatMap(element => generateCases(element, coreByName, workdir));

		const rendered = renderCorpus(cases);

		if (checkMode) {
			let existing: string;
			try {
				existing = readFileSync(outPath, 'utf8');
			} catch {
				existing = '';
			}
			if (existing !== rendered) {
				console.error(
					`generated corpus is stale -> ${displayPath(outPath)}\n`
						+ 'run `bun run gen:corpus` to regenerate.',
				);
				process.exitCode = 1;
				return;
			}
			console.log(`generated corpus up to date -> ${displayPath(outPath)}`);
			console.log(`cases: ${cases.length} (${elements.length} elements, all element attributes and sampled variants)`);
			return;
		}

		writeFileSync(outPath, rendered);
		console.log(`generated corpus written -> ${displayPath(outPath)}`);
		console.log(`source -> ${displayPath(corePath)}`);
		console.log(`cases: ${cases.length} (${elements.length} elements, all element attributes and sampled variants)`);
	} finally {
		rmSync(workdir, { recursive: true, force: true });
	}
}

if (import.meta.main) main();
