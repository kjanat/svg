import { expect, test } from 'bun:test';
import { attributeSamples, cssGrammarSample, elementSnippets } from './gen-corpus.ts';

type Attribute = Parameters<typeof attributeSamples>[0];
type Values = NonNullable<Attribute['values']>;
type Element = Parameters<typeof elementSnippets>[0];
type Accepts = (value: string) => boolean;
type SampleCase = { values: Values; forms: Accepts[] };

const matches = (pattern: RegExp): Accepts => value => pattern.test(value);
const literal = (expected: string): Accepts => value => value === expected;
const number = matches(/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i);
const integer = matches(/^[+-]?\d+$/);
const length = matches(/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:px|em|rem|cm|mm|in|pt|pc)?$/);
const percentage = matches(/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)%$/);
const angle = matches(/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:deg|rad|grad|turn)$/);
const identifier = matches(/^[a-z_][\w-]*$/i);
const color = matches(/^(?:red|blue|green|#[\da-f]{3}(?:[\da-f]{3})?)$/i);
const iri = matches(/^url\(#[\w-]+\)$/);
const path = matches(/^M\s*[-\d.]+[ ,]+[-\d.]+(?:\s*L\s*[-\d.]+[ ,]+[-\d.]+)+$/);
const any = (...forms: Accepts[]): Accepts => value => forms.some(accepts => accepts(value));
const list = (item: Accepts, separator: RegExp, min = 2, max = Infinity): Accepts => value => {
	const items = value.split(separator);
	return items.length >= min && items.length <= max && items.every(item);
};
const pair = list(number, /\s*,\s*/, 2, 2);
const pixels = matches(/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)px$/);

function assertForms(samples: string[], forms: Accepts[]): void {
	expect(samples).not.toBeEmpty();
	expect(samples).toHaveLength(new Set(samples).size);
	for (const value of samples) expect(value).toSatisfy(any(...forms));
	for (const [index, accepts] of forms.entries()) expect(samples, `missing value form ${index}`).toSatisfy(values => values.some(accepts));
}

// Adding a catalog kind requires a fixture here. CSS grammars and unsupported
// values have their own parameterized tables below.
const valueCases = {
	enum: { values: { kind: 'enum', values: ['second', 'first', 'second'] }, forms: [literal('first'), literal('second')] },
	transform: { values: { kind: 'transform', functions: ['translate'] }, forms: [matches(/^translate\([\d.]+[ ,]+[\d.]+\)$/)] },
	color: { values: { kind: 'color' }, forms: [color] },
	paint: { values: { kind: 'paint' }, forms: [color] },
	length: { values: { kind: 'length' }, forms: [length] },
	url: { values: { kind: 'url' }, forms: [matches(/^#[\w-]+$/)] },
	boolean: { values: { kind: 'boolean' }, forms: [literal('false'), literal('true')] },
	token_list: { values: { kind: 'token_list' }, forms: [list(identifier, /\s+/)] },
	comma_token_list: { values: { kind: 'comma_token_list' }, forms: [list(identifier, /\s*,\s*/)] },
	url_token_list: { values: { kind: 'url_token_list' }, forms: [list(matches(/^#[\w-]+$/), /\s+/)] },
	language_tag: { values: { kind: 'language_tag' }, forms: [matches(/^[a-z]{2,3}(?:-[a-z\d]+)*$/i)] },
	integer: { values: { kind: 'integer' }, forms: [integer] },
	number: { values: { kind: 'number' }, forms: [number] },
	id_list: { values: { kind: 'id_list' }, forms: [list(identifier, /\s+/)] },
	media_type: { values: { kind: 'media_type' }, forms: [matches(/^[\w.+-]+\/[\w.+-]+$/)] },
	media_query_list: { values: { kind: 'media_query_list' }, forms: [literal('screen')] },
	css_declaration_list: { values: { kind: 'css_declaration_list' }, forms: [matches(/^fill:\s*(?:red|blue|green);?$/)] },
	id: { values: { kind: 'id' }, forms: [identifier] },
	referrer_policy: { values: { kind: 'referrer_policy' }, forms: [literal('no-referrer')] },
	suggested_file_name: { values: { kind: 'suggested_file_name' }, forms: [matches(/^[\w-]+\.svg$/)] },
	path_data: { values: { kind: 'path_data' }, forms: [path] },
	semicolon_number_list: { values: { kind: 'semicolon_number_list' }, forms: [list(number, /\s*;\s*/)] },
	coordinate_pair: { values: { kind: 'coordinate_pair' }, forms: [pair] },
	coordinate_pair_list: { values: { kind: 'coordinate_pair_list' }, forms: [list(pair, /\s*;\s*/)] },
	number_or_percentage: { values: { kind: 'number_or_percentage' }, forms: [number, percentage] },
	free_text: { values: { kind: 'free_text' }, forms: [value => value.length > 0] },
} satisfies Record<Exclude<Values['kind'], 'css_grammar' | 'unresolved'>, SampleCase>;

test.each(Object.entries(valueCases))('catalog kind %s produces valid samples for every supported form', (_, fixture) => {
	assertForms(attributeSamples({ name: 'example', values: fixture.values }, 'svg'), fixture.forms);
});

function css(grammar: string, keywords: string[] = []): Extract<Values, { kind: 'css_grammar' }> {
	return { kind: 'css_grammar', grammar, graph: { nodes: keywords.map(text => ({ kind: 'keyword', text })) } };
}

const blendModes = [
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
];
const cssCases: [string, string[], Accepts[]][] = [
	['<number>', [], [number]],
	['<integer>', [], [integer]],
	['<length>', [], [number, pixels]],
	['<length-percentage>', [], [number, pixels, percentage]],
	['<angle>', [], [angle]],
	['<number-optional-number>', [], [number, list(number, /\s+/, 2, 2)]],
	['<points>', [], [pair, list(pair, /\s+/)]],
	['<blend-mode>', [], blendModes.map(literal)],
	['<filter-primitive-reference>', [], [identifier]],
	['<XML-Name>', [], [identifier]],
	['<string>', [], [path]],
	['<value>', [], [number]],
	['<url>', [], [iri]],
	['<iri>', [], [iri]],
	['<color>', [], [color]],
	['<paint>', [], [color, literal('none'), iri]],
	['[<min-x>,? <min-y>,? <width>,? <height>]', [], [list(number, /\s+/, 4, 4), list(number, /\s*,\s*/, 4, 4)]],
	['<align> <meetOrSlice>?', [], [literal('none'), literal('xMidYMid'), literal('xMidYMid meet'), literal('xMidYMid slice')]],
	['none', ['none'], [literal('none')]],
	['none | auto', ['none', 'auto'], [literal('none'), literal('auto')]],
	['auto | <length-percentage>', ['auto'], [number, pixels, percentage, literal('auto')]],
	['<length-percentage> | auto', ['auto'], [number, pixels, percentage, literal('auto')]],
	['auto | auto-start-reverse | <angle> | <number>', ['auto', 'auto-start-reverse'], [literal('auto'), literal('auto-start-reverse'), angle, number]],
	['[ anonymous | use-credentials ]?', ['anonymous', 'use-credentials'], [literal('anonymous'), literal('use-credentials')]],
	[
		'<length-percentage> | <number> | left | center | right',
		['left', 'center', 'right'],
		[number, pixels, percentage, literal('left'), literal('center'), literal('right')],
	],
	['none | <string>', ['none'], [literal('none'), path]],
	['<number> | <number>', [], [number]],
	['none | none', ['none', 'none'], [literal('none')]],
	['<number>+ | <angle>', [], [number, list(number, /\s+/), angle]],
	['<angle> | <number>#', [], [angle, number, list(number, /\s*,\s*/)]],
	['[ <number> | <integer> ]+ | <angle>', [], [number, list(number, /\s+/), angle]],
];

test.each(cssCases)('CSS grammar %s preserves every alternative', (grammar, keywords, forms) => {
	const values = css(grammar, keywords);
	const samples = attributeSamples({ name: 'example', values }, 'svg');
	assertForms(samples, forms);
	const representative = cssGrammarSample(values);
	expect(representative).toBeOneOf(samples);
});

const repeatable: [string, Accepts][] = [
	['<number>', number],
	['<integer>', integer],
	['<length>', length],
	['<length-percentage>', any(length, percentage)],
	['<angle>', angle],
];
const listCases: [string, Accepts[]][] = repeatable.flatMap(([type, item]): [string, Accepts[]][] => [
	[`${type}+`, [item, list(item, /\s+/)]],
	[`${type}#`, [item, list(item, /\s*,\s*/)]],
	[`[ ${type}+ ]#`, [item, list(item, /\s+/), list(item, /\s*,\s*/)]],
]);
listCases.push(['[ [ <length-percentage> | <number> ]+ ]#', [
	number,
	pixels,
	percentage,
	list(any(length, percentage), /\s+/),
	list(any(length, percentage), /\s*,\s*/),
]]);

test.each(listCases)('CSS list grammar %s retains scalar and list forms', (grammar, forms) => {
	assertForms(attributeSamples({ name: 'example', values: css(grammar) }, 'svg'), forms);
});

test.each([
	['script', 'type', 'application/ecmascript'],
	['style', 'type', 'text/css'],
	['a', 'type', 'text/html'],
	['svg', 'type', 'text/css'],
	['script', 'other', 'text/css'],
])('%s.%s selects the appropriate media type', (element, name, expected) => {
	expect(attributeSamples({ name, values: { kind: 'media_type' } }, element)).toEqual([expected]);
});

test.each(
	[
		['matching override', 'text', { kind: 'number' }, ['auto']],
		['nonmatching override', 'svg', { kind: 'number' }, null],
		['override without default', 'text', undefined, ['auto']],
	] satisfies [string, string, Values | undefined, string[] | null][],
)('%s respects element-specific values', (_, element, values, expected) => {
	const samples = attributeSamples(
		{ name: 'example', values, element_values: [{ element: 'text', values: { kind: 'enum', values: ['auto'] } }] },
		element,
	);
	if (expected) expect(samples).toEqual(expected);
	else assertForms(samples, [number]);
});

const errorCases: [string, Attribute, string][] = [
	['missing values', { name: 'example' }, 'No samples for svg.example'],
	['unresolved kind', { name: 'example', values: { kind: 'unresolved' } }, 'No samples for svg.example'],
	['empty enum', { name: 'example', values: { kind: 'enum', values: [] } }, 'No samples for svg.example'],
	['unknown CSS type', { name: 'example', values: css('<unknown>') }, 'No samples for <unknown>'],
	['empty CSS grammar', { name: 'example', values: css('') }, 'No samples for svg.example'],
	['unclosed CSS group', { name: 'example', values: css('[ <number>+') }, 'Unbalanced CSS grammar'],
	['unexpected closing CSS group', { name: 'example', values: css('<number> ]') }, 'Unbalanced CSS grammar'],
	['unknown catalog kind', { name: 'example', values: { kind: 'future_type' } } as unknown as Attribute, 'Unsupported catalog values'],
	[
		'unresolved override',
		{ name: 'example', values: { kind: 'number' }, element_values: [{ element: 'svg', values: { kind: 'unresolved' } }] },
		'No samples for svg.example',
	],
];
test.each(errorCases)('%s fails instead of silently losing coverage', (_, attribute, message) => {
	expect(() => attributeSamples(attribute, 'svg')).toThrow(message);
});

test.each(['svg', 'g'])('%s without attributes retains its bare case', name => {
	expect(elementSnippets({ name }, new Map()).map(testCase => testCase.input)).toEqual([name === 'svg' ? '<svg/>' : '<svg><g/></svg>']);
});

test('a missing catalog attribute fails with its element and name', () => {
	expect(() => elementSnippets({ name: 'svg', attrs: ['missing'] }, new Map())).toThrow('svg.missing');
});

const catalog = await Bun.file(new URL('../../../crates/svg-data/data/catalog.core.json', import.meta.url)).json() as {
	elements: Element[];
	attributes: Attribute[];
};
const attributes = new Map(catalog.attributes.map(attr => [attr.name, attr]));
const corpus = await Bun.file(new URL('../test/corpus/generated_catalog_elements.txt', import.meta.url)).text();
const inputs = new Set(corpus.split('\n').filter(line => line.startsWith('<svg')));
const catalogCases = catalog.elements.flatMap(element =>
	(element.attrs ?? []).map(name => ({ label: `${element.name}.${name}`, element, attribute: attributes.get(name)! }))
);

test.each(catalogCases)('$label retains every sampled variant in the corpus', ({ element, attribute }) => {
	expect(attribute).toBeDefined();
	const samples = attributeSamples(attribute, element.name);
	const snippets = new Set(elementSnippets(element, attributes).map(testCase => testCase.input));
	for (const value of samples) {
		const input = element.name === 'svg' ? `<svg ${attribute.name}="${value}"/>` : `<svg><${element.name} ${attribute.name}="${value}"/></svg>`;
		expect(snippets).toContain(input);
		expect(inputs).toContain(input);
	}
	const values = attribute.element_values?.find(entry => entry.element === element.name)?.values ?? attribute.values;
	if (values?.kind === 'enum') expect(new Set(samples)).toEqual(new Set(values.values));
});

test.each(['dx', 'dy'].flatMap(name => ['feDropShadow', 'feOffset', 'text', 'tspan'].map(element => [element, name])))(
	'%s.%s uses the element-specific scalar/list definition',
	(element, name) => {
		const samples = attributeSamples(attributes.get(name)!, element);
		if (element === 'feDropShadow' || element === 'feOffset') assertForms(samples, [number]);
		else assertForms(samples, [number, pixels, percentage, list(any(length, percentage), /\s+/), list(any(length, percentage), /\s*,\s*/)]);
	},
);

test.each(catalog.elements)('$name snippets are deterministic, unique and cover all attributes together', element => {
	const snippets = elementSnippets(element, attributes);
	for (const snippet of snippets) expect(inputs).toContain(snippet.input);
	expect(snippets).toHaveLength(new Set(snippets.map(testCase => testCase.input)).size);
	expect(elementSnippets({ ...element, attrs: [...(element.attrs ?? [])].reverse() }, attributes)).toEqual(snippets);
	for (const name of element.attrs ?? []) expect(snippets[0]!.input).toContain(` ${name}="`);
});
