import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('../actions/append-release-notes/run.sh', import.meta.url));

function runScript(root: string, draft: string, calls: string) {
	return spawnSync('bash', [script], {
		cwd: root,
		encoding: 'utf8',
		env: {
			...process.env,
			PATH: `${root}:${process.env.PATH}`,
			RELEASE_TAG: 'v0.4.0',
			GITHUB_REPOSITORY: 'fixture/svg',
			GH_TOKEN: 'fixture',
			GH_CALLS: calls,
			DRAFT_BODY: draft,
		},
	});
}

function withFixture(
	changelog: string,
	check: (fixture: {
		run: () => ReturnType<typeof runScript>;
		draft: string;
		calls: string;
	}) => void,
) {
	const root = mkdtempSync(join(tmpdir(), 'svg-release-notes-'));
	try {
		const draft = join(root, 'draft-body');
		const calls = join(root, 'gh-calls');
		writeFileSync(join(root, 'CHANGELOG.md'), changelog);
		writeFileSync(draft, 'Existing draft body.');
		writeFileSync(
			join(root, 'gh'),
			`#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$GH_CALLS"
case "$1 $2" in
  'api repos/fixture/svg/releases/generate-notes') printf '%s\n' 'Generated fixture notes.' ;;
  'release view')
    test ! -e "$DRAFT_BODY.unavailable"
    cat "$DRAFT_BODY" ;;
  'release edit')
    while [ "$#" -gt 0 ]; do
      if [ "$1" = --notes-file ]; then cp "$2" "$DRAFT_BODY"; exit 0; fi
      shift
    done
    exit 1 ;;
  *) echo 'Unexpected GitHub operation' >&2; exit 1 ;;
esac
`,
		);
		chmodSync(join(root, 'gh'), 0o700);
		const run = () => runScript(root, draft, calls);
		check({ run, draft, calls });
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
}

test('a missing changelog section preserves draft notes and adds generated notes once across retries', () => {
	withFixture('## [Unreleased]\nFuture changes.\n## [0.3.0]\nOlder changes.\n', ({ run, draft }) => {
		for (let attempt = 0; attempt < 2; attempt++) expect(run().status).toBe(0);
		const body = readFileSync(draft, 'utf8');
		expect(body).toContain('Existing draft body.');
		expect(body).not.toContain('Future changes.');
		expect(body).not.toContain('Older changes.');
		expect(body.match(/generated-notes-start/g)).toHaveLength(1);
		expect(body.match(/Generated fixture notes\./g)).toHaveLength(1);
	});
});

test('a failed draft read preserves the draft without generating or writing notes', () => {
	withFixture('## [Unreleased]\n', ({ run, draft, calls }) => {
		writeFileSync(`${draft}.unavailable`, '');
		expect(run().status).not.toBe(0);
		expect(readFileSync(draft, 'utf8')).toBe('Existing draft body.');
		const operations = readFileSync(calls, 'utf8');
		expect(operations).toContain('release view');
		expect(operations).not.toContain('generate-notes');
		expect(operations).not.toContain('release edit');
	});
});

test('a matching section preserves curated notes and remains stable on retry', () => {
	withFixture(
		'## [Unreleased]\nFuture changes.\n## [0.4.0] - 2026-11-15\n### Fixed\nCurated release text.\n## [0.3.0]\nOlder changes.\n',
		({ run, draft, calls }) => {
			for (let attempt = 0; attempt < 2; attempt++) expect(run().status).toBe(0);
			const body = readFileSync(draft, 'utf8');
			expect(body).toContain('Curated release text.');
			expect(body).not.toContain('Future changes.');
			expect(body).not.toContain('Older changes.');
			expect(body.match(/generated-notes-start/g)).toHaveLength(1);
			expect(body.match(/Generated fixture notes\./g)).toHaveLength(1);
			expect(readFileSync(calls, 'utf8').split('\n').filter(line => line.startsWith('release edit'))).toHaveLength(2);
		},
	);
});

test('an existing empty section retains the existing generated-notes behavior', () => {
	withFixture('## [0.4.0]\n\n## [0.3.0]\nOlder changes.\n', ({ run, draft }) => {
		expect(run().status).toBe(0);
		expect(readFileSync(draft, 'utf8')).toContain('Generated fixture notes.');
	});
});
