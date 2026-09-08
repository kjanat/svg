/**
 * Pure unit tests for the lib. No HTTP, no live network — fast and
 * deterministic. Exercises the parse layer directly with synthetic
 * inputs so each rule is covered without depending on whatever
 * shape web-features happens to ship today.
 *
 * @module
 */
// @ts-nocheck Deno

import { buildSnapshot } from '#lib/build.ts';
import {
	_resetLoggedWarnings,
	extractBrowserSupport,
	parseBaseline,
	parseBaselineDate,
	parseBrowserVersion,
	selectBrowserStatement,
} from '#lib/parse.ts';
import type { BaselineDate } from '#lib/types.ts';
import { assertEquals, assertExists } from '@std/assert';

Deno.test('parseBaselineDate maps known qualifier prefixes', () => {
	_resetLoggedWarnings();
	const cases: Array<[string, BaselineDate['qualifier'], string]> = [
		['≤2021-04-02', 'before', '2021-04-02'],
		['<2020-01-01', 'before', '2020-01-01'],
		['<=2020-01-01', 'before', '2020-01-01'],
		['≥2024-06-01', 'after', '2024-06-01'],
		['>2024-06-01', 'after', '2024-06-01'],
		['>=2024-06-01', 'after', '2024-06-01'],
		['~2023-08-15', 'approximately', '2023-08-15'],
		['2022-12-31', undefined, '2022-12-31'],
	];
	for (const [raw, qualifier, date] of cases) {
		const got = parseBaselineDate(raw, 'test.fixture');
		assertExists(got, `expected BaselineDate for ${raw}`);
		assertEquals(got?.raw, raw);
		assertEquals(got?.date, date);
		assertEquals(got?.qualifier, qualifier);
	}
});

Deno.test('parseBaselineDate preserves raw on completely unparseable input', () => {
	_resetLoggedWarnings();
	const got = parseBaselineDate('garbage', 'test.fixture');
	assertExists(got);
	assertEquals(got?.raw, 'garbage');
	assertEquals(got?.date, undefined);
	assertEquals(got?.qualifier, undefined);
});

Deno.test('parseBaselineDate preserves unknown prefixes without assigning a meaning', () => {
	_resetLoggedWarnings();
	const got = parseBaselineDate('%2024-01-01', 'test.fixture');
	assertExists(got);
	assertEquals(got?.raw, '%2024-01-01');
	assertEquals(got?.date, undefined);
	assertEquals(got?.qualifier, undefined);
});

Deno.test('parseBaselineDate retains empty strings and omits absent dates', () => {
	_resetLoggedWarnings();
	assertEquals(parseBaselineDate(undefined, 'test.fixture'), undefined);
	assertEquals(parseBaselineDate('', 'test.fixture'), { raw: '' });
});

Deno.test('parseBaseline preserves raw on unparseable date but baseline tier is known', () => {
	_resetLoggedWarnings();
	const got = parseBaseline(
		{
			baseline: 'high',
			baseline_high_date: 'garbage',
			baseline_low_date: 'garbage',
		},
		'test.fixture',
	);
	assertExists(got);
	assertEquals(got?.status, 'widely');
	assertEquals(got?.high_date?.raw, 'garbage');
	assertEquals(got?.high_date?.date, undefined);
	assertEquals(Object.hasOwn(got, 'since'), false);
});

Deno.test('parseBaseline never discards on unknown baseline value', () => {
	_resetLoggedWarnings();
	const got = parseBaseline({ baseline: 'experimental' }, 'test.fixture');
	assertExists(got);
	assertEquals(got?.status, undefined);
	assertEquals(got?.status_diagnostic, 'unrecognized');
	assertEquals(got?.raw_status, '"experimental"');
});

Deno.test('parseBaseline maps known prefix end-to-end on real-world feGaussianBlur shape', () => {
	_resetLoggedWarnings();
	// Mirror the exact shape we get from web-features 3.23.0
	// for `svg.elements.feGaussianBlur` via by_compat_key.
	const got = parseBaseline(
		{
			baseline: 'high',
			baseline_high_date: '≤2021-04-02',
			baseline_low_date: '≤2018-10-02',
		},
		'svg.elements.feGaussianBlur',
	);
	assertExists(got);
	assertEquals(got?.status, 'widely');
	assertEquals(got?.high_date?.date, '2021-04-02');
	assertEquals(got?.high_date?.qualifier, 'before');
	assertEquals(got?.high_date?.raw, '≤2021-04-02');
	assertEquals(got?.high_date?.date, '2021-04-02');
	assertEquals(got?.high_date?.qualifier, 'before');
	assertEquals(got?.low_date?.raw, '≤2018-10-02');
	assertEquals(got?.low_date?.date, '2018-10-02');
	assertEquals(got?.low_date?.qualifier, 'before');
});

Deno.test('parseBaseline returns limited with preserved dates when baseline === false', () => {
	_resetLoggedWarnings();
	const got = parseBaseline(
		{
			baseline: false,
			baseline_low_date: '2024-01-01',
		},
		'test.fixture',
	);
	assertExists(got);
	assertEquals(got?.status, 'limited');
	assertEquals(got?.low_date?.date, '2024-01-01');
});

Deno.test('parseBaseline returns undefined only when there is no upstream data at all', () => {
	_resetLoggedWarnings();
	assertEquals(parseBaseline({}, 'test.fixture'), undefined);
});

Deno.test('parseBrowserVersion preserves concrete version string', () => {
	_resetLoggedWarnings();
	const got = parseBrowserVersion({ version_added: '50' }, 'chrome', 'test.fixture');
	assertExists(got);
	assertEquals(got?.version_added, '50');
});

Deno.test('parseBrowserVersion extracts ≤ qualifier on version strings', () => {
	_resetLoggedWarnings();
	const got = parseBrowserVersion({ version_added: '≤50' }, 'chrome', 'test.fixture');
	assertExists(got);
	assertEquals(got?.version_added, '≤50');
});

Deno.test('parseBrowserVersion preserves explicit false (the glyph-orientation-horizontal case)', () => {
	_resetLoggedWarnings();
	const got = parseBrowserVersion({ version_added: false }, 'chrome', 'test.fixture');
	assertExists(got);
	assertEquals(got?.version_added, false);
});

Deno.test('parseBrowserVersion keeps invalid true neutral', () => {
	_resetLoggedWarnings();
	const got = parseBrowserVersion({ version_added: true }, 'chrome', 'test.fixture');
	assertExists(got);
	assertEquals(got?.version_added, undefined);
});

Deno.test('parseBrowserVersion preserves null', () => {
	_resetLoggedWarnings();
	const got = parseBrowserVersion({ version_added: null }, 'chrome', 'test.fixture');
	assertExists(got);
	assertEquals(got?.version_added, undefined);
});

Deno.test('parseBrowserVersion surfaces version_removed with qualifier', () => {
	_resetLoggedWarnings();
	const got = parseBrowserVersion(
		{ version_added: '22', version_removed: '≤120' },
		'chrome',
		'test.fixture',
	);
	assertExists(got);
	assertEquals(got?.version_added, '22');
	assertEquals(got?.version_removed, '≤120');
});

Deno.test('parseBrowserVersion preserves partial_implementation, prefix, alternative_name', () => {
	_resetLoggedWarnings();
	const got = parseBrowserVersion(
		{
			version_added: '80',
			partial_implementation: true,
			prefix: '-webkit-',
			alternative_name: 'foo-bar',
		},
		'chrome',
		'test.fixture',
	);
	assertExists(got);
	assertEquals(got?.partial_implementation, true);
	assertEquals(got?.prefix, '-webkit-');
	assertEquals(got?.alternative_name, 'foo-bar');
});

Deno.test('parseBrowserVersion normalises notes (string → string[])', () => {
	_resetLoggedWarnings();
	const single = parseBrowserVersion(
		{ version_added: '50', notes: 'only partial' },
		'chrome',
		'test.fixture',
	);
	assertEquals(single?.notes, ['only partial']);

	const array = parseBrowserVersion(
		{ version_added: '50', notes: ['a', 'b'] },
		'chrome',
		'test.fixture',
	);
	assertEquals(array?.notes, ['a', 'b']);
});

Deno.test('parseBrowserVersion validates flag shapes and drops malformed entries', () => {
	_resetLoggedWarnings();
	const got = parseBrowserVersion(
		{
			version_added: '50',
			flags: [
				{ type: 'preference', name: 'layout.css.foo' },
				{ type: 'runtime_flag', name: 'enable-foo', value_to_set: 'true' },
				{ type: 'preference' }, // missing name → dropped + warned
			],
		},
		'firefox',
		'test.fixture',
	);
	assertExists(got);
	assertExists(got?.flags);
	assertEquals(got?.flags?.length, 2);
	assertEquals(got?.flags?.[0], { type: 'preference', name: 'layout.css.foo' });
	assertEquals(got?.flags?.[1], {
		type: 'runtime_flag',
		name: 'enable-foo',
		value_to_set: 'true',
	});
});

Deno.test('browser histories retain old statements while selection chooses unrestricted support', () => {
	const support = extractBrowserSupport({
		support: {
			chrome: [
				{
					version_added: '20',
					version_removed: '70',
					version_last: '69',
					prefix: '-webkit-',
					impl_url: ['https://example.com/1', 'https://example.com/2'],
				},
				{ version_added: '80' },
			],
			safari_ios: { version_added: '18.4' },
			future_device: { version_added: false },
		},
	}, 'test.fixture');
	assertEquals(support?.chrome.length, 2);
	assertEquals(support?.chrome[0].version_last, '69');
	assertEquals(support?.chrome[0].impl_url, ['https://example.com/1', 'https://example.com/2']);
	assertEquals(selectBrowserStatement(support?.chrome)?.version_added, '80');
	assertEquals(support?.safari_ios[0].version_added, '18.4');
	assertEquals(support?.future_device[0].version_added, false);
});

Deno.test('parseBrowserVersion returns undefined only for truly absent data', () => {
	_resetLoggedWarnings();
	assertEquals(parseBrowserVersion(undefined, 'chrome', 'test.fixture'), undefined);
});

Deno.test('shared fixture preserves full browser facts and independently selected Web Features support', () => {
	const fixture = JSON.parse(Deno.readTextFileSync(new URL('../../../../crates/svg-data/src/fixtures/browser-support.json', import.meta.url)));
	const support = extractBrowserSupport(fixture, 'svg.elements.rect');
	assertEquals(Object.keys(support ?? {}).length, 5);
	assertEquals(support?.chrome.length, 2);
	const original = support?.chrome[0];
	assertEquals(original?.version_added, '≤20');
	assertEquals(original?.version_last, '69');
	assertEquals(original?.flags, fixture.support.chrome[0].flags);
	assertEquals(original?.impl_url, fixture.support.chrome[0].impl_url);
	assertEquals(original?.notes, fixture.support.chrome[0].notes);
	assertEquals(selectBrowserStatement(support?.chrome)?.version_added, '80');
	const baseline = parseBaseline(fixture.status.by_compat_key['svg.elements.rect'], 'svg.elements.rect');
	assertEquals(baseline?.support, { chrome: '80', safari_ios: '18.4' });
});

Deno.test('attribute summaries compare qualified versions without losing exact histories', () => {
	const context = (version: string) => ({
		__compat: { support: { chrome: [{ version_added: '10', version_removed: '20' }, { version_added: version }] } },
	});
	const snapshot = buildSnapshot({
		sources: {},
		featureMap: {},
		svgRoot: {
			elements: {
				rect: { width: context('≤50') },
				svg: { width: context('60') },
			},
		},
	});
	const attribute = snapshot.attributes.width;
	assertEquals(attribute.contexts['svg.elements.rect.width'].browser_support?.chrome[1].version_added, '≤50');
	assertEquals(attribute.contexts['svg.elements.svg.width'].browser_support?.chrome.length, 2);
	assertEquals(selectBrowserStatement(attribute.aggregate.browser_support?.chrome)?.version_added, '60');
});
