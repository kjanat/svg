import { expect, test } from 'bun:test';
import { versionEdits } from '../../scripts/release-version.mjs';
import { carriers, workspaceVersion } from '../actions/version-check/carriers.mjs';

const load = async name => Bun.YAML.parse(await Bun.file(new URL(`../workflows/${name}.yml`, import.meta.url)).text());
const release = await load('release');
const npm = await load('npm-release');
const crates = await load('crates-release');
const refresh = await load('refresh-spec');
const prepare = await load('prepare-release');
const promote = release;
const configuration = await Bun.file(new URL('../beepboop.json', import.meta.url)).json();
const targets = await Bun.file(new URL('../../distribution/npm/targets.json', import.meta.url)).json();
const stream = configuration.streams.main;
const steps = job => job.steps ?? [];
const action = (job, name) => steps(job).find(step => step.uses?.startsWith(`kjanat/beepboop/actions/${name}@`));
const dependencies = job => [job.needs ?? []].flat();

test('version PRs stay manual and promotion requires approval and a signed source', () => {
	expect(stream.prepare.versionLands).toBe('via-version-pr');
	expect(stream.prepare.versionPrMerge).toBe('manual');
	expect(stream.promote.publishWhen).toBe('approved-in-beepboop');
	expect(stream.promote.workflow.refs).toMatchObject({ phase: 'version', prefix: 'v', signed: true });
	expect(stream.prepare.strategy).toBe('workflow');
	expect(stream.startFrom).toBeUndefined();
	expect(action(prepare.jobs.prepare, 'pull-request').with.base).toBe('master');
	const verification = steps(promote.jobs['create-release']).findIndex(step => step.run?.includes('.commit.verification.verified'));
	const publish = steps(promote.jobs['create-release']).findIndex(step => step.uses?.includes('/actions/publish-refs@'));
	expect(verification).toBeGreaterThanOrEqual(0);
	expect(publish).toBeGreaterThan(verification);
});

test('every supported platform publication is required and experimental packages stay optional', () => {
	for (const target of targets.targets) {
		for (const facade of targets.facades) {
			const publication = `pkg:npm/${encodeURIComponent(targets.scope)}/${facade.pkg}-${target.pkg}`;
			expect(stream.packages).toContain(publication);
			expect(stream.mustPublish.includes(publication)).toBe(!target.experimental);
		}
	}
	expect(stream.packages.filter(value => value.startsWith('pkg:cargo/'))).toHaveLength(12);
	expect(stream.promote.workflow.publications).toContain('pkg:github/kjanat/svg');
});

test('the release operation reaches every writer and both reusable publishers', () => {
	for (const name of ['create-release', 'build-and-upload', 'build-dist', 'publish-release']) {
		const job = release.jobs[name];
		expect(dependencies(job)).toContain('beepboop');
		expect(action(job, 'token').with['execution-id']).toBe('${{ needs.beepboop.outputs.execution-id }}');
		expect(job.permissions).toEqual({ contents: 'read', 'id-token': 'write' });
	}
	for (const name of ['call-npm-release', 'call-crates-release']) {
		expect(dependencies(release.jobs[name])).toContain('publish-release');
		expect(release.jobs[name].with['beepboop-execution-id']).toBe('${{ needs.beepboop.outputs.execution-id }}');
		expect(release.jobs[name].secrets).toBeUndefined();
	}
});

test('registry credentials come from scoped Beepboop grants after verification', () => {
	expect(stream.credentials).toEqual(['crates-io', 'npm-token']);
	expect(dependencies(npm.jobs.beepboop)).toEqual(['setup', 'runtime-smoke']);
	expect(crates.jobs.beepboop.needs).toBe('setup');
	for (const name of ['grammars', 'scoped', 'facades', 'shims', 'bundles']) {
		const job = npm.jobs[name];
		expect(dependencies(job)).toContain('beepboop');
		expect(job.environment.name).toBe('npm');
		expect(action(job, 'credentials').with.aliases).toBe('["npm-token"]');
		expect(action(job, 'credentials').if).toBe("needs.setup.outputs.dry-run != 'true'");
	}
	expect(action(crates.jobs.publish, 'credentials').with.aliases).toBe('["crates-io"]');
	expect(crates.jobs.publish.if).toBe('inputs.dry-run != true');
	const source = JSON.stringify([release, npm, crates, refresh]);
	expect(source).not.toMatch(/secrets\.(NPM_TOKEN|CARGO_REGISTRY_TOKEN|SPEC_REFRESH_PAT)/);
});

test('refresh PRs use App identity, remain unmerged and preserve regeneration checks', () => {
	const job = refresh.jobs.mutate;
	expect(job.permissions).toEqual({ contents: 'read', 'id-token': 'write' });
	expect(action(job, 'token').with['permission-issues']).toBe('write');
	expect(action(job, 'pull-request').with.branch).toBe('spec-data/auto-refresh');
	expect(steps(job).some(step => step.run?.includes('just regen $check.head_commit'))).toBe(true);
	expect(JSON.stringify(job)).not.toMatch(/gh pr merge|--auto|SPEC_REFRESH_PAT|peter-evans/);
});

test('workflow preparation updates every version carrier without tags or commits', async () => {
	const edits = await versionEdits('0.5.0');
	expect(workspaceVersion(edits.get('Cargo.toml'))).toBe('0.5.0');
	for (const carrier of carriers) {
		const content = edits.get(carrier.file);
		if (content === undefined) continue;
		const matches = carrier.all ? [...content.matchAll(carrier.pattern)] : [content.match(carrier.pattern)];
		expect(matches.length).toBeGreaterThan(0);
		for (const match of matches) expect(match?.[1]).toBe('0.5.0');
	}
	expect(edits.size).toBe(28);
	await expect(versionEdits('0.5.0\nmalformed')).rejects.toThrow('Expected a semantic version');
});

test('release publication preserves the Beepboop draft and composed release notes', () => {
	expect(JSON.stringify(release)).not.toContain('create-gh-release-action');
	const finalSteps = steps(release.jobs['publish-release']);
	const publish = finalSteps.findIndex(step => step.run?.includes('--draft=false'));
	const record = finalSteps.findIndex(step => step.uses?.includes('/actions/publish-release@'));
	expect(publish).toBeGreaterThanOrEqual(0);
	expect(record).toBeGreaterThan(publish);
});
