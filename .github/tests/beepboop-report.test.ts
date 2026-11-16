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
	outputs?: Record<string, string>;
	steps: Step[];
}
interface Workflow {
	jobs: Record<string, Job>;
}
interface Context {
	matrix: { crate?: string; pkg?: string; experimental?: boolean };
	steps: { publish: { outcome: string; outputs: Record<string, string> } };
	needs: { setup: { outputs: Record<string, string> }; beepboop: { outputs: Record<string, string> } };
}

const root = new URL('../..', import.meta.url).pathname;
const scratch = mkdtempSync(join(tmpdir(), 'beepboop-report-'));
afterAll(() => rmSync(scratch, { recursive: true, force: true }));

const load = async (name: string): Promise<Workflow> =>
	Bun.YAML.parse(await Bun.file(join(root, `.github/workflows/${name}.yml`)).text()) as unknown as Workflow;
const workflows = { 'crates-release': await load('crates-release'), 'npm-release': await load('npm-release') };

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

const evaluate = (expression: string, context: Context): unknown =>
	new Function('matrix', 'steps', 'needs', `return (${expression.replaceAll('.package-name', "['package-name']")});`)(
		context.matrix,
		context.steps,
		context.needs,
	);
const render = (template: string, context: Context): string =>
	template.replace(/\$\{\{(.*?)\}\}/g, (_, expression: string) => String(evaluate(expression, context)));

const reporting = [
	['crates-release', 'publish'],
	['npm-release', 'grammars'],
	['npm-release', 'scoped'],
	['npm-release', 'facades'],
	['npm-release', 'shims'],
	['npm-release', 'bundles'],
] as const;

function reportStep(file: keyof typeof workflows, name: string): Step {
	const job = workflows[file].jobs[name];
	if (!job) throw new Error(`Missing job ${file}.${name}`);
	const step = job.steps.at(-1);
	if (!step) throw new Error(`Empty job ${file}.${name}`);
	return step;
}

function publications(file: keyof typeof workflows, name: string, context: Partial<Context> & { outcome?: string; skipped?: string }) {
	const template = reportStep(file, name).with?.publications;
	if (typeof template !== 'string') throw new Error(`${file}.${name} reports no publications`);
	return JSON.parse(render(template, {
		matrix: { crate: 'svg-lint', pkg: 'tree-sitter-svg', experimental: false, ...context.matrix },
		steps: {
			publish: {
				outcome: context.outcome ?? 'success',
				outputs: { 'package-name': '@svg-toolkit/svg-lint', ...(context.skipped ? { skipped: context.skipped } : {}) },
			},
		},
		needs: { setup: { outputs: { version: '1.2.3' } }, beepboop: { outputs: { version: '1.2.3' } } },
	}));
}

test('every publishing job reports to Beepboop directly after the publish step, whatever its result', () => {
	for (const [file, name] of reporting) {
		const job = workflows[file].jobs[name];
		if (!job) throw new Error(`Missing job ${file}.${name}`);
		const publish = job.steps.findIndex(step => step.id === 'publish');
		const credentials = job.steps.findIndex(step => step.id === 'credentials');
		const step = reportStep(file, name);
		expect([name, credentials < publish, publish < job.steps.length - 1]).toEqual([name, true, true]);
		expect(step.uses).toBe('kjanat/beepboop/actions/report@v1');
		expect(step.if).toBe("${{ !cancelled() && steps.credentials.outcome == 'success' }}");
		expect(step.with?.['execution-id']).toBe('${{ needs.beepboop.outputs.execution-id }}');
	}
	expect(workflows['npm-release'].jobs.beepboop?.outputs?.version).toBe('${{ steps.version.outputs.version }}');
});

test('reports name each package by manager and name with the version from the release tag', () => {
	expect(publications('crates-release', 'publish', {})).toEqual([{
		id: 'cargo:svg-lint',
		manager: 'cargo',
		name: 'svg-lint',
		version: '1.2.3',
		required: true,
		outcome: 'succeeded',
	}]);
	expect(publications('npm-release', 'grammars', {})[0]).toMatchObject({ id: 'npm:tree-sitter-svg', name: 'tree-sitter-svg' });
	for (const name of ['scoped', 'facades', 'shims', 'bundles']) {
		expect(publications('npm-release', name, {})).toEqual([{
			id: 'npm:@svg-toolkit/svg-lint',
			manager: 'npm',
			name: '@svg-toolkit/svg-lint',
			version: '1.2.3',
			required: true,
			outcome: 'succeeded',
		}]);
	}
});

test('reports map the publish step result to the Beepboop outcome', () => {
	const outcome = (file: keyof typeof workflows, name: string, context: Parameters<typeof publications>[2]) =>
		publications(file, name, context)[0].outcome;
	for (const [file, name] of reporting) {
		expect([name, outcome(file, name, { outcome: 'failure' })]).toEqual([name, 'failed']);
		expect([name, outcome(file, name, { outcome: 'cancelled' })]).toEqual([name, 'failed']);
	}
	const experimental = { matrix: { experimental: true } };
	expect(outcome('npm-release', 'scoped', { ...experimental, skipped: 'true' })).toBe('skipped');
	expect(outcome('npm-release', 'scoped', { skipped: 'true' })).toBe('failed');
	expect(outcome('npm-release', 'scoped', { ...experimental, outcome: 'failure' })).toBe('failed');
	expect(publications('npm-release', 'scoped', experimental)[0].required).toBe(false);
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
