import { afterAll, expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

interface Step {
	id?: string;
	name?: string;
	uses?: string;
	if?: string;
	with?: Record<string, unknown>;
}
interface Job {
	steps: Step[];
}
interface Workflow {
	jobs: Record<string, Job>;
}

const root = new URL('../..', import.meta.url).pathname;
const scratch = mkdtempSync(join(tmpdir(), 'beepboop-report-'));
afterAll(() => rmSync(scratch, { recursive: true, force: true }));

const load = async (name: string): Promise<Workflow> =>
	Bun.YAML.parse(await Bun.file(join(root, `.github/workflows/${name}.yml`)).text()) as unknown as Workflow;
const crates = await load('crates-release');
const npm = await load('npm-release');

let runs = 0;
function run(script: string, env: Record<string, string>, cwd = root) {
	const output = join(scratch, `output-${runs++}`);
	writeFileSync(output, '');
	const result = Bun.spawnSync(['bash', join(root, script)], {
		cwd,
		env: { PATH: process.env.PATH ?? '', GITHUB_OUTPUT: output, ...env },
	});
	const values = Object.fromEntries(
		readFileSync(output, 'utf8').split('\n').filter(Boolean).map(line => {
			const at = line.indexOf('=');
			return [line.slice(0, at), line.slice(at + 1)];
		}),
	);
	return { exitCode: result.exitCode, values, stderr: result.stderr.toString() };
}

const report = (env: Record<string, string>) =>
	run('.github/actions/beepboop-report/run.sh', {
		MANAGER: 'npm',
		PACKAGE_NAME: '@svg-toolkit/svg-lint',
		VERSION: 'v1.2.3',
		RESULT: 'success',
		SKIPPED: 'false',
		REQUIRED: 'true',
		...env,
	});
const publications = (env: Record<string, string>) => JSON.parse(report(env).values.publications);

test('a report names the package by its purl and the version without the tag prefix', () => {
	expect(publications({})).toEqual([{
		id: 'pkg:npm/%40svg-toolkit/svg-lint',
		manager: 'npm',
		name: '@svg-toolkit/svg-lint',
		version: '1.2.3',
		outcome: 'succeeded',
		required: true,
	}]);
	expect(publications({ MANAGER: 'cargo', PACKAGE_NAME: 'svg-lint', VERSION: '1.2.3' })[0])
		.toMatchObject({ id: 'pkg:cargo/svg-lint', manager: 'cargo', version: '1.2.3' });
});

test('a report maps the publish step result to the Beepboop outcome', () => {
	const outcome = (env: Record<string, string>) => publications(env)[0].outcome;
	expect(outcome({ RESULT: 'success' })).toBe('succeeded');
	expect(outcome({ RESULT: 'failure' })).toBe('failed');
	expect(outcome({ RESULT: 'cancelled' })).toBe('failed');
	expect(outcome({ RESULT: 'success', SKIPPED: 'true', REQUIRED: 'false' })).toBe('skipped');
	expect(outcome({ RESULT: 'success', SKIPPED: 'true', REQUIRED: 'true' })).toBe('failed');
	expect(publications({ REQUIRED: 'false' })[0].required).toBe(false);
});

test('a report refuses an unknown manager or required flag', () => {
	expect(report({ MANAGER: 'pypi' })).toMatchObject({ exitCode: 1, values: {} });
	expect(report({ REQUIRED: 'yes' })).toMatchObject({ exitCode: 1, values: {} });
});

test('npm matrix publishing names its package and marks a missing optional platform skipped', () => {
	const publish = (only: string) =>
		run('.github/actions/npm-publish/run.sh', {
			RELEASE_TAG: 'v1.2.3',
			DIST_TAG: 'latest',
			DRY_RUN: 'true',
			REGISTRY: 'https://registry.npmjs.org/',
			ONLY_PACKAGE: only,
			GITHUB_WORKSPACE: root,
		}, scratch);
	expect(publish('lint-freebsd-arm64')).toMatchObject({
		exitCode: 0,
		values: { 'package-name': '@svg-toolkit/lint-freebsd-arm64', skipped: 'true' },
	});
	const facade = publish('@svg-toolkit/svg-format');
	expect([facade.exitCode, facade.values]).toEqual([1, { 'package-name': '@svg-toolkit/svg-format' }]);
	const platform = publish('format-linux-x64-gnu');
	expect([platform.exitCode, platform.values]).toEqual([1, { 'package-name': '@svg-toolkit/format-linux-x64-gnu' }]);
});

const reported = [
	['crates-release', 'publish', 'cargo', '${{ matrix.crate }}', '${{ needs.setup.outputs.version }}'],
	['npm-release', 'grammars', 'npm', '${{ matrix.pkg }}', '${{ env.RELEASE_TAG }}'],
	['npm-release', 'scoped', 'npm', '${{ steps.publish.outputs.package-name }}', '${{ env.RELEASE_TAG }}'],
	['npm-release', 'facades', 'npm', '${{ steps.publish.outputs.package-name }}', '${{ env.RELEASE_TAG }}'],
	['npm-release', 'shims', 'npm', '${{ steps.publish.outputs.package-name }}', '${{ env.RELEASE_TAG }}'],
	['npm-release', 'bundles', 'npm', '${{ steps.publish.outputs.package-name }}', '${{ env.RELEASE_TAG }}'],
] as const;

test('every publishing job reports its package after the publish step, whatever its result', () => {
	for (const [file, name, manager, packageName, version] of reported) {
		const job = (file === 'npm-release' ? npm : crates).jobs[name];
		if (!job) throw new Error(`Missing job ${file}.${name}`);
		const publish = job.steps.findIndex(step => step.id === 'publish');
		const credentials = job.steps.findIndex(step => step.id === 'credentials');
		const index = job.steps.findIndex(step => step.uses?.endsWith('/.github/actions/beepboop-report'));
		expect([name, credentials < publish, publish < index, index === job.steps.length - 1]).toEqual([name, true, true, true]);
		const step = job.steps[index];
		expect(step?.if).toBe("${{ !cancelled() && steps.credentials.outcome == 'success' }}");
		expect(step?.with).toMatchObject({
			'execution-id': '${{ needs.beepboop.outputs.execution-id }}',
			manager,
			name: packageName,
			version,
			result: '${{ steps.publish.outcome }}',
		});
	}
	expect(npm.jobs.scoped?.steps.at(-1)?.with).toMatchObject({
		skipped: "${{ steps.publish.outputs.skipped || 'false' }}",
		required: '${{ !matrix.experimental }}',
	});
});
