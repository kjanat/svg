import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { after, test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { GlobalConfig } from './node_modules/renovate/dist/config/global.js';
import { init } from './node_modules/renovate/dist/logger/index.js';
import { extractPackageFile as regexExtract } from './node_modules/renovate/dist/modules/manager/custom/regex/index.js';
import { updateArtifacts } from './node_modules/renovate/dist/modules/manager/pep621/artifacts.js';
import { extractPackageFile as pepExtract } from './node_modules/renovate/dist/modules/manager/pep621/extract.js';
import { extractPackageFile as pyenvExtract } from './node_modules/renovate/dist/modules/manager/pyenv/extract.js';
import { api as pep440 } from './node_modules/renovate/dist/modules/versioning/pep440/index.js';
import { api as python } from './node_modules/renovate/dist/modules/versioning/python/index.js';
import { setCustomEnv } from './node_modules/renovate/dist/util/env.js';
import { applyPackageRules } from './node_modules/renovate/dist/util/package-rules/index.js';
import { doAutoReplace } from './node_modules/renovate/dist/workers/repository/update/branch/auto-replace.js';
await init();
// Renovate passes only an allowlisted environment to `uv lock`; without this it ignores the warmed cache.
if (process.env.UV_CACHE_DIR) setCustomEnv({ UV_CACHE_DIR: process.env.UV_CACHE_DIR });
const root = fileURLToPath(new URL('../../../', import.meta.url));
const scratch = await mkdtemp(join(tmpdir(), 'svg-python-renovate-'));
const current = (await readFile(join(root, '.python-version'), 'utf8')).trim();
const [major, minor] = current.split('.').map(Number);
const config = JSON.parse(await readFile(join(root, 'renovate.json'), 'utf8'));
const releasePattern = new RegExp(config.packageRules.find(r => r.extractVersion).extractVersion);
test('Python prereleases are excluded and patch releases normalize to major/minor', () => {
	for (const suffix of ['a1', 'b2', 'rc1']) assert.equal(releasePattern.test(`${major}.${minor + 1}.0${suffix}`), false);
	assert.equal(releasePattern.exec(`${current}.9`).groups.version, current);
});
const grammars = (await readdir(join(root, 'grammars'))).filter(n => /^tree-sitter-svg(?:-|$)/.test(n));
const files = ['.python-version', 'pyproject.toml', ...grammars.map(n => `grammars/${n}/pyproject.toml`)];
for (const release of [`${major}.${minor + 1}.2`, `${major + 1}.0.1`, `${major}.${minor}.9`]) {
	test(`all Python targets stay coordinated for ${release}`, async () => {
		const localDir = join(scratch, release);
		GlobalConfig.set({ localDir, cacheDir: join(scratch, 'cache'), platform: 'github', binarySource: 'global', executionTimeout: 5 });
		const contents = new Map();
		for (const file of [...files, 'uv.lock']) {
			const content = await readFile(join(root, file), 'utf8');
			contents.set(file, content);
			await mkdir(dirname(join(localDir, file)), { recursive: true });
			await writeFile(join(localDir, file), content);
		}
		const updates = [];
		async function add(manager, file, extraction, managerConfig = {}) {
			for (const [depIndex, dep] of (extraction?.deps ?? []).entries()) {
				const depName = dep.depName ?? dep.packageName;
				if (depName !== 'python') continue;
				if (manager === 'pep621') assert.deepEqual(extraction.lockFiles, ['uv.lock']);
				const update = await applyPackageRules({
					...managerConfig,
					...extraction,
					...dep,
					depName,
					packageName: dep.packageName ?? depName,
					manager,
					packageFile: file,
					depIndex,
					packageRules: config.packageRules,
				});
				assert.equal(update.datasource, 'python-version');
				assert.equal(update.groupName, 'Python');
				assert.equal(update.rangeStrategy, 'bump');
				assert.equal(update.groupSlug, 'python');
				assert.equal(update.separateMajorMinor, false);
				assert.equal(update.separateMinorPatch, false);
				const version = new RegExp(update.extractVersion).exec(release).groups.version;
				assert.equal(version, release.split('.').slice(0, 2).join('.'));
				const versioning = update.versioning === 'pep440' ? pep440 : python;
				update.newValue = versioning.getNewValue({
					currentValue: update.currentValue,
					rangeStrategy: update.rangeStrategy,
					currentVersion: current,
					newVersion: version,
				});
				assert.equal(update.newValue, manager === 'pep621' ? `>=${version}` : version);
				updates.push(update);
			}
		}
		await add('pyenv', '.python-version', pyenvExtract(contents.get('.python-version')));
		for (const file of files.filter(f => f.includes('pyproject'))) {
			await add('pep621', file, await pepExtract(contents.get(file), file, {}));
			for (const custom of config.customManagers) {
				if (!custom.managerFilePatterns.some(p => new RegExp(p.slice(1, -1)).test(file))) continue;
				await add('regex', file, regexExtract(contents.get(file), file, custom), custom);
			}
		}
		assert.equal(updates.length, 12);
		for (const update of updates) {
			const before = contents.get(update.packageFile);
			const after = await doAutoReplace(update, before, false);
			assert.ok(after, `${update.manager}: ${update.packageFile}`);
			contents.set(update.packageFile, after);
		}
		const version = release.split('.').slice(0, 2).join('.');
		for (const file of files) {
			const original = await readFile(join(root, file), 'utf8');
			const expected = original.replaceAll(current, version)
				.replaceAll(`py${major}${minor}`, `py${version.replace('.', '')}`)
				.replaceAll(`cp${major}${minor}`, `cp${version.replace('.', '')}`);
			assert.equal(contents.get(file), expected, file);
		}
	});
}

test('native package minimum updates regenerate the workspace uv lockfile', { timeout: 120_000 }, async () => {
	const localDir = join(scratch, 'lockfile');
	GlobalConfig.set({ localDir, cacheDir: join(scratch, 'cache'), platform: 'github', binarySource: 'global', executionTimeout: 5 });
	for (const file of [...files, 'uv.lock']) {
		const content = await readFile(join(root, file), 'utf8');
		await mkdir(dirname(join(localDir, file)), { recursive: true });
		const baseline = file === 'uv.lock' || file.startsWith('grammars/')
			? content.replace(`requires-python = ">=${current}"`, `requires-python = ">=${major}.${minor - 1}"`)
			: content;
		await writeFile(join(localDir, file), baseline);
	}
	const packageFileName = `grammars/${grammars[0]}/pyproject.toml`;
	const newPackageFileContent = await readFile(join(root, packageFileName), 'utf8');
	const previousPackageFileContent = await readFile(join(localDir, packageFileName), 'utf8');
	assert.notEqual(previousPackageFileContent, newPackageFileContent);
	const extracted = await pepExtract(previousPackageFileContent, packageFileName, {});
	const dep = extracted.deps.find(d => (d.depName ?? d.packageName) === 'python');
	assert.ok(dep);
	assert.deepEqual(extracted.lockFiles, ['uv.lock']);
	const artifacts = await updateArtifacts({
		packageFileName,
		newPackageFileContent,
		config: { constraints: { python: current } },
		updatedDeps: [{ ...dep, newValue: `>=${current}`, newVersion: current }],
	});
	assert.equal(artifacts?.length, 1, JSON.stringify(artifacts));
	assert.equal(artifacts[0].file.path, 'uv.lock');
	assert.ok(artifacts[0].file.contents.includes(`requires-python = ">=${current}"`));
	execFileSync('uv', ['lock', '--check', '--project', localDir], { stdio: 'pipe' });
});

after(() => rm(scratch, { recursive: true, force: true }));
