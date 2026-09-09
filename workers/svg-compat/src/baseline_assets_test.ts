import { buildOutput } from '#lib/build.ts';
import type { Baseline } from '#lib/types.ts';
import server from '#server';
import { renderHtml } from '#src/render.tsx';
import { assert, assertEquals, assertStringIncludes } from '@std/assert';
import manifest from '../../../crates/svg-language-server/assets/baseline-icons.json' with { type: 'json' };

function badgePage(baseline: Baseline | undefined): string {
	const source = { package: 'fixture', requested: '1', resolved: '1', mode: 'default' as const, source_url: 'https://example.com/' };
	return renderHtml(
		buildOutput({
			sources: { bcd: source, web_features: source },
			elements: { rect: { baseline, deprecated: false, experimental: false, standard_track: true, spec_url: [] } },
			attributes: {},
		}, '2026-09-09T00:00:00Z'),
		new URL('http://localhost/'),
	);
}

Deno.test('each Baseline tier selects the official light and dark assets', () => {
	for (const status of ['widely', 'newly', 'limited'] as const) {
		const html = badgePage({ status });
		assertStringIncludes(html, '<picture class="badge-icon">');
		assertStringIncludes(html, 'media="(prefers-color-scheme: dark)"');
		assertStringIncludes(html, `srcset="/badges/baseline-${status}-icon-dark.svg"`);
		assertStringIncludes(html, `src="/badges/baseline-${status}-icon.svg"`);
		assertStringIncludes(html, 'width="18" height="10"');
	}
	for (const baseline of [undefined, {}, { raw_status: '"future"' }]) {
		const html = badgePage(baseline);
		assertStringIncludes(html, 'Unknown');
		assert(!html.includes('<picture'));
		assert(!html.includes('/badges/'));
	}
});

Deno.test('all served Baseline variants match the pinned canonical bytes', async () => {
	for (const [name, hash] of Object.entries(manifest.sha256)) {
		const source = await Deno.readFile(new URL(`../../../crates/svg-language-server/assets/${name}`, import.meta.url));
		const response = await server.fetch(new Request(`http://localhost/badges/${name}`));
		assertEquals(response.status, 200, name);
		assert(response.headers.get('content-type')?.startsWith('image/svg+xml'));
		const served = new Uint8Array(await response.arrayBuffer());
		assertEquals(served, source, name);
		const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', served));
		assertEquals(Array.from(digest, byte => byte.toString(16).padStart(2, '0')).join(''), hash, name);
	}
});

Deno.test('asset preflight rejects drift and sync repairs only generated copies', async () => {
	const root = await Deno.makeTempDir({ prefix: 'svg-baseline-assets-' });
	const original = `${root}/crates/svg-language-server/assets`;
	const generated = `${root}/workers/svg-compat/static/badges`;
	try {
		await Deno.mkdir(`${root}/scripts`, { recursive: true });
		await Deno.mkdir(original, { recursive: true });
		await Deno.mkdir(generated, { recursive: true });
		await Deno.copyFile(new URL('../../../scripts/baseline-icons.ts', import.meta.url), `${root}/scripts/baseline-icons.ts`);
		await Deno.copyFile(
			new URL('../../../crates/svg-language-server/assets/baseline-icons.json', import.meta.url),
			`${original}/baseline-icons.json`,
		);
		for (const name of Object.keys(manifest.sha256)) {
			await Deno.copyFile(new URL(`../../../crates/svg-language-server/assets/${name}`, import.meta.url), `${original}/${name}`);
		}
		const run = (mode: string) =>
			new Deno.Command('deno', {
				args: ['run', '--no-config', '--allow-read', '--allow-write', `${root}/scripts/baseline-icons.ts`, mode],
				stdout: 'piped',
				stderr: 'piped',
			}).output();
		const synced = await run('--sync');
		assertEquals(synced.code, 0, new TextDecoder().decode(synced.stderr));
		assertEquals((await run('--check')).code, 0);
		const name = 'baseline-widely-icon.svg';
		await Deno.writeTextFile(`${generated}/${name}`, 'modified');
		assertEquals((await run('--check')).success, false);
		assertEquals((await run('--sync')).code, 0);
		await Deno.remove(`${generated}/${name}`);
		assertEquals((await run('--check')).success, false);
		assertEquals((await run('--sync')).code, 0);
		await Deno.writeTextFile(`${generated}/baseline-obsolete.svg`, 'old copy');
		assertEquals((await run('--check')).success, false);
		await Deno.remove(`${generated}/baseline-obsolete.svg`);
		await Deno.writeTextFile(`${original}/${name}`, 'modified original');
		assertEquals((await run('--check')).success, false);
		assertEquals((await run('--sync')).success, false);
		assertEquals(
			await Deno.readFile(`${generated}/${name}`),
			await Deno.readFile(new URL(`../../../crates/svg-language-server/assets/${name}`, import.meta.url)),
		);
	} finally {
		await Deno.remove(root, { recursive: true });
	}
});
