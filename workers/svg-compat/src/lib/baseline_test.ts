import { buildOutput, buildSnapshot } from '#lib/build.ts';
import { extractBaseline, makeCompatEntry, parseBaseline } from '#lib/parse.ts';
import { SVG_COMPAT_SCHEMA } from '#lib/schema.ts';
import type { Baseline, CompatEntry } from '#lib/types.ts';
import { countBuckets } from '#src/cli_render.ts';
import { renderHtml } from '#src/render.tsx';
import { assert, assertEquals, assertStringIncludes } from '@std/assert';
import fixture from '../../../../crates/svg-data/src/fixtures/web-features.json' with { type: 'json' };

const source = { package: 'fixture', requested: '1', resolved: '1', mode: 'default' as const, source_url: 'https://example.com/fixture' };
const page = (entry: CompatEntry) =>
	renderHtml(
		buildOutput({ sources: { bcd: source, web_features: source }, elements: { fixture: entry }, attributes: {} }, '2026-01-01T00:00:00Z'),
		new URL('https://example.com/'),
		false,
	);

const jsonValue = (value: unknown) => JSON.parse(JSON.stringify(value ?? null));

Deno.test('attribute aggregation uses the selected tier milestone and retains advice contexts', () => {
	const snapshot = buildSnapshot({
		bcdRoot: {},
		svgRoot: { elements: { a: { width: { __compat: {} } }, b: { width: { __compat: {} } }, c: { width: { __compat: {} } } } },
		sources: { bcd: source, web_features: source },
		featureMap: {
			fixture: {
				kind: 'feature',
				compat_features: ['svg.elements.a.width', 'svg.elements.b.width', 'svg.elements.c.width'],
				discouraged: { reason: 'Feature-scoped advice', according_to: [], alternatives: ['svg'] },
				status: {
					by_compat_key: {
						'svg.elements.a.width': { baseline: 'low', baseline_low_date: '2021-01-01', baseline_high_date: '2025-01-01' },
						'svg.elements.b.width': { baseline: 'low', baseline_low_date: '2024-01-01', baseline_high_date: '2023-01-01' },
						'svg.elements.c.width': { baseline: null, baseline_low_date: '2028-01-01' },
					},
				},
			},
		},
	});
	assertEquals(snapshot.attributes.width.contexts['svg.elements.a.width'].baseline?.low_date?.date, '2021-01-01');
	assertEquals(snapshot.attributes.width.contexts['svg.elements.a.width'].discouraged?.length, 1);
	assertEquals(snapshot.attributes.width.contexts['svg.elements.c.width'].baseline?.status, undefined);
	assertEquals(snapshot.attributes.width.coverage, { contexts: 3, baseline_known: 2, baseline_unknown: 1, baseline_missing: 0 });
	assertEquals(snapshot.attributes.width.aggregate.baseline?.status, 'newly');
	assertEquals(snapshot.attributes.width.aggregate.baseline?.low_date?.date, '2024-01-01');
	assertEquals(snapshot.attributes.width.aggregate.baseline?.high_date?.date, '2023-01-01');
	assertEquals(snapshot.attributes.width.aggregate.discouraged?.map(item => item.compat_key), [
		'svg.elements.a.width',
		'svg.elements.b.width',
		'svg.elements.c.width',
	]);
});

Deno.test('shared Web Features fixtures survive parsing, serialization, and presentation', () => {
	const entries: Record<string, CompatEntry> = {};
	for (const test of fixture.cases) {
		const entry = makeCompatEntry(
			{},
			{ fixture: { kind: 'feature', compat_features: ['svg.elements.fixture'], status: test.input } },
			'svg.elements.fixture',
		);
		entries[test.name] = entry;
		assertEquals(jsonValue(entry.baseline), test.expected, test.name);
		const restored = jsonValue(entry.baseline) as Baseline | null;
		const html = page({ ...entry, baseline: restored ?? undefined });
		if (!restored?.status) {
			assertStringIncludes(html, 'Unknown', test.name);
			assert(!html.includes('src="/badges/'), test.name);
			assert(!html.includes('badge-limited'), test.name);
		} else if (test.name === 'both-milestones') {
			assertStringIncludes(html, 'Widely Available since 2022');
			assertStringIncludes(html, 'Newly Available since 2020-01-15');
			assertStringIncludes(html, 'Widely Available since 2022-07-15');
			assert(!html.includes('Widely Available since 2020'));
		} else if (test.name === 'newly-with-both-dates') {
			assertStringIncludes(html, 'Newly Available since 2020');
		} else if (test.name === 'malformed-dates') {
			assertStringIncludes(html, 'Widely Available');
			assertStringIncludes(html, 'date not recognized');
			assert(!html.includes('since 2021'));
		}
	}
	assertEquals(countBuckets(entries), { widely: 5, newly: 4, limited: 1, unknown: 6, total: 16 });
});

Deno.test('per-key status wins without inheriting dates or tier from the feature', () => {
	const features = fixture.features;
	assertEquals(extractBaseline({}, features, 'svg.elements.rect')?.high_date?.date, '2022-07-15');
	assertEquals(jsonValue(extractBaseline({}, features, 'svg.elements.rect.width')), { status: 'newly', raw_status: '"low"' });
	assertEquals(extractBaseline({}, features, 'svg.elements.rect.opacity'), undefined);
	assertEquals(extractBaseline({}, features, 'svg.elements.rect.height')?.status, undefined);
	assertEquals(extractBaseline({}, features, 'svg.elements.rect.height')?.raw_status, 'null');
});

Deno.test('discouragement replaces the badge and remains distinct from BCD deprecation', () => {
	const discouraged = makeCompatEntry({}, fixture.features, 'svg.elements.legacy');
	assertEquals(discouraged.baseline?.status, 'limited');
	assertEquals(discouraged.deprecated, false);
	assertEquals(discouraged.discouraged?.[0].scope, 'feature');
	assertEquals(discouraged.discouraged?.[0].compat_key, 'svg.elements.legacy');
	assertEquals(discouraged.discouraged?.[0].alternatives, ['svg']);
	const html = page(discouraged);
	assertStringIncludes(html, 'WebDX discourages Legacy SVG');
	assertStringIncludes(html, 'Use a modern SVG feature.');
	assertStringIncludes(html, 'https://example.com/retirement');
	assertStringIncludes(html, 'Alternatives: svg');
	assert(!html.includes('src="/badges/'));
	assert(!html.includes('<em>'));
	const limited = makeCompatEntry({}, fixture.features, 'svg.elements.limited');
	assertEquals(limited.baseline?.status, 'limited');
	assertEquals(limited.discouraged, undefined);
	const deprecated = makeCompatEntry({ status: { deprecated: true } }, fixture.features, 'svg.elements.deprecated');
	assertEquals(deprecated.deprecated, true);
	assertEquals(deprecated.discouraged, undefined);
	assertEquals(deprecated.baseline?.status, 'widely');
});

Deno.test('versioned worker output and schema expose optional status and both dates', () => {
	const source = { package: 'fixture', requested: '1', resolved: '1', mode: 'default' as const, source_url: 'https://example.com/fixture' };
	const entry = makeCompatEntry({}, fixture.features, 'svg.elements.rect');
	const data = jsonValue(
		buildOutput({ sources: { bcd: source, web_features: source }, elements: { rect: entry }, attributes: {} }, '2026-01-01T00:00:00Z'),
	);
	assertEquals(data.schema_version, 2);
	assertEquals(data.elements.rect.baseline.low_date.date, '2020-01-15');
	assertEquals(data.elements.rect.baseline.high_date.date, '2022-07-15');
	assertEquals(SVG_COMPAT_SCHEMA.properties.schema_version.const, 2);
	assertEquals(SVG_COMPAT_SCHEMA.$defs.baseline.required, []);
	assertEquals(SVG_COMPAT_SCHEMA.$defs.baseline.properties.raw_status.type, 'string');
	assertEquals(parseBaseline({ baseline: undefined }, 'fixture'), undefined);
});
