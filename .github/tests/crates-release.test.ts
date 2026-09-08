import { expect, test } from 'bun:test';

interface Step {
	id?: string;
	uses?: string;
	shell?: string;
	run?: string;
	with?: Record<string, unknown>;
	env?: Record<string, unknown>;
	if?: string;
	'continue-on-error'?: boolean;
}
interface Job {
	needs?: string;
	if?: string;
	permissions: Record<string, string>;
	outputs?: Record<string, string>;
	steps: Step[];
}
interface Workflow {
	on: Record<string, { inputs: Record<string, { type: string; default?: unknown }> }>;
	jobs: Record<string, Job>;
}

const workflow = Bun.YAML.parse(await Bun.file(new URL('../workflows/crates-release.yml', import.meta.url)).text()) as unknown as Workflow;
const { setup, publish } = workflow.jobs;
const checkouts = (job: Job) => job.steps.filter(step => step.uses?.startsWith('actions/checkout@'));
const step = (job: Job, id: string) => {
	const found = job.steps.find(item => item.id === id);
	if (!found) throw new Error(`Missing step ${id}`);
	return found;
};

test('both entry points verify packages and permit a no-upload dry run', () => {
	for (const event of ['workflow_call', 'workflow_dispatch']) {
		expect(workflow.on[event].inputs.tag.type).toBe('string');
		expect(workflow.on[event].inputs['dry-run']).toMatchObject({ type: 'boolean', default: false });
	}
	expect(setup.if).toBeUndefined();
	expect(publish.needs).toBe('setup');
	expect(publish.if).toBe('inputs.dry-run != true');
	expect(setup.permissions).toEqual({ contents: 'read' });
	expect(publish.permissions).toEqual({ contents: 'read', 'id-token': 'write' });
});

test('helpers resolve from the default branch once and publish uses that commit', () => {
	expect(checkouts(setup)[0].with?.ref).toBe('${{ github.event.repository.default_branch }}');
	expect(checkouts(publish)[0].with?.ref).toBe('${{ needs.setup.outputs.helper_sha }}');
	for (const job of [setup, publish]) {
		const [helper, source] = checkouts(job);
		expect(helper.with).toMatchObject({ 'persist-credentials': false, 'sparse-checkout': '.github/actions', 'sparse-checkout-cone-mode': false });
		expect(source.with).toEqual({ ref: 'refs/tags/${{ env.RELEASE_TAG }}', 'persist-credentials': false, path: 'source' });
		expect(step(job, 'identity').shell).toBe('python');
		expect(job.steps.find(item => item.uses?.startsWith('dtolnay/rust-toolchain@'))?.with?.toolchain)
			.toBe('${{ steps.identity.outputs.toolchain }}');
	}
	expect(step(publish, 'identity').env).toEqual({
		EXPECTED_SOURCE_SHA: '${{ needs.setup.outputs.source_sha }}',
		EXPECTED_HELPER_SHA: '${{ needs.setup.outputs.helper_sha }}',
	});
	expect(setup.outputs?.source_sha).toBe('${{ steps.identity.outputs.source_sha }}');
	expect(setup.outputs?.helper_sha).toBe('${{ steps.identity.outputs.helper_sha }}');
});

test('only successful package verification can supply the publication matrix and artifacts', () => {
	const packages = step(setup, 'packages');
	const artifact = step(setup, 'artifact');
	expect(packages.uses).toBe('./.github/actions/crates-verify');
	expect(packages['continue-on-error']).toBeUndefined();
	expect(packages.if).toBeUndefined();
	expect(artifact.if).toBeUndefined();
	expect(setup.steps.indexOf(packages)).toBeLessThan(setup.steps.indexOf(artifact));
	expect(setup.outputs?.crates).toBe('${{ steps.packages.outputs.crates }}');
	expect(setup.outputs?.artifact_id).toBe('${{ steps.artifact.outputs.artifact-id }}');
	expect(publish.steps.find(item => item.uses?.startsWith('actions/download-artifact@'))?.with).toMatchObject({
		'artifact-ids': '${{ needs.setup.outputs.artifact_id }}',
		path: packages.with?.['proof-dir'],
	});
	expect(publish.steps.find(item => item.uses === './.github/actions/crates-publish')?.with).toMatchObject({
		'proof-dir': packages.with?.['proof-dir'],
		tag: '${{ env.RELEASE_TAG }}',
		'helper-sha': '${{ needs.setup.outputs.helper_sha }}',
	});
});
