import { expect, test } from 'bun:test';

const load = async (name: string): Promise<any> => Bun.YAML.parse(await Bun.file(new URL(`../workflows/${name}.yml`, import.meta.url)).text());
const release = await load('release');
const npm = await load('npm-release');
const runtime = await load('runtime-smoke');
const ci = await load('runtime-smoke-ci');
const dependencies = (job: any): string[] => [job.needs ?? []].flat();
const stepsText = (job: any) => job.steps.map((s: any) => s.run ?? s.uses ?? '').join('\n');

test('GitHub release publication waits for the required runtime gate', () => {
	expect(dependencies(release.jobs['runtime-smoke'])).toEqual(['build-dist']);
	expect(dependencies(release.jobs['publish-release'])).toContain('runtime-smoke');
	expect(release.jobs['runtime-smoke']['continue-on-error']).toBeUndefined();
	expect(dependencies(release.jobs['call-npm-release'])).toContain('publish-release');
	expect(dependencies(release.jobs['call-crates-release'])).toContain('publish-release');
});

test('every npm entry point gates both publishing roots, preserving publication order', () => {
	expect(Object.keys(npm.on).sort()).toEqual(['release', 'workflow_call', 'workflow_dispatch']);
	expect(dependencies(npm.jobs['runtime-smoke'])).toEqual(['setup']);
	for (const name of ['grammars', 'scoped']) expect(dependencies(npm.jobs[name])).toContain('runtime-smoke');
	expect(dependencies(npm.jobs.facades)).toContain('scoped');
	expect(dependencies(npm.jobs.shims)).toContain('facades');
	expect(dependencies(npm.jobs.bundles)).toContain('shims');
	expect(runtime.on.workflow_call.inputs['helper-ref']).toBeUndefined();
});

test('runtime checks use transferred artifacts, native runners and a real musl container', () => {
	const smoke = runtime.jobs.smoke;
	expect(smoke['runs-on']).toBe('${{ matrix.runner }}');
	expect(smoke['continue-on-error']).toBeUndefined();
	expect(smoke.strategy['fail-fast']).toBe(false);
	expect(stepsText(smoke)).not.toMatch(/cargo|build-packages|gh release download/);
	expect(stepsText(smoke)).toContain('docker run --rm');
	expect(stepsText(smoke)).toContain('node --test distribution/npm/facade/test/*.test.mjs');
	for (const job of [runtime.jobs.matrix, smoke]) {
		for (const step of job.steps.filter((s: any) => s.uses?.startsWith('actions/setup-node@'))) {
			expect(step.with['package-manager-cache']).toBe(false);
		}
	}
	for (const step of smoke.steps.filter((s: any) => s.uses?.startsWith('actions/checkout@'))) {
		expect(step.with.ref).toBe('${{ github.sha }}');
		expect(step.with['persist-credentials']).toBe(false);
	}
});

test('both artifact producers preserve archives and modes before the shared runtime workflow', () => {
	for (const job of [release.jobs['build-dist'], ci.jobs.package]) {
		expect(stepsText(job)).toContain('bun install --frozen-lockfile --ignore-scripts');
		expect(stepsText(job)).toContain('tar -cf distribution/npm/dist.tar -C distribution/npm dist downloads');
		expect(stepsText(job)).toContain('runtime.mjs inventory');
	}
	expect(dependencies(ci.jobs.runtime)).toEqual(['package']);
	expect(stepsText(ci.jobs.package)).not.toMatch(/cargo build|npm publish|gh release create/);
	expect(ci.permissions).toEqual({ contents: 'read', actions: 'read' });
});
