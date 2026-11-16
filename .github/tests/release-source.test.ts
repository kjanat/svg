import { expect, test } from 'bun:test';

function record(value: unknown): Record<string, unknown> {
	if (typeof value !== 'object' || value === null || Array.isArray(value)) throw new Error('Expected a mapping');
	return Object.fromEntries(Object.entries(value));
}
function steps(job: Record<string, unknown>): Record<string, unknown>[] {
	const list = job.steps;
	if (!Array.isArray(list)) throw new Error('Expected steps');
	return list.map(record);
}

const jobs = record(
	record(Bun.YAML.parse(await Bun.file(new URL('../workflows/release.yml', import.meta.url)).text())).jobs,
);
const job = (name: string) => record(jobs[name]);
const source = '${{ needs.beepboop.outputs.source-sha }}';
const checkouts = (name: string) =>
	steps(job(name)).filter(step => typeof step.uses === 'string' && step.uses.startsWith('actions/checkout@'))
		.map(step => record(step.with));

test('every job that builds or packages the release checks out the approved source commit', () => {
	for (const name of ['verify', 'matrix', 'build-and-upload', 'build-dist']) {
		expect(checkouts(name).map(options => options.ref)).toEqual([source]);
	}
	expect(record(job('version-check').with).ref).toBe(source);
});

test('the draft release job reads the dispatched workflow commit with the full history', () => {
	expect(checkouts('create-release').map(options => [options.ref, options['fetch-depth']])).toEqual([
		['${{ github.sha }}', 0],
	]);
});
