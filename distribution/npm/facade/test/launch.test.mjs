import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import process from 'node:process';
import { after, test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { BIN, FOREIGN, HOST, makeFixture, removeFixtures } from './fixture.mjs';

after(removeFixtures);

const posixOnly = { skip: process.platform === 'win32' && 'POSIX signal semantics only' };
const env = { ...process.env, SVG_LIBC: 'glibc' };

/** @param {string} root */
const facade = (root) => join(root, 'bin', `${BIN}.mjs`);

/**
 * Run a facade whose binary is the given shell script.
 *
 * @param {string} script
 * @param {string[]} [args]
 */
function launch(script, args = []) {
	const root = makeFixture([HOST], { script: `#!/bin/sh\n${script}\n` });
	return spawnSync(process.execPath, [facade(root), ...args], { env, encoding: 'utf8', timeout: 30_000 });
}

test("the binary's exit status and arguments pass through", posixOnly, () => {
	const result = launch('printf "%s|" "$@"; exit 7', ['--version', 'two words']);
	assert.deepEqual([result.status, result.signal, result.stdout], [7, null, '--version|two words|']);
});

test('a binary killed by a signal kills the launcher with the same signal', posixOnly, () => {
	const result = launch('kill -TERM $$');
	assert.deepEqual([result.status, result.signal], [null, 'SIGTERM']);
});

test('a signal Node ignores or claims still ends the launcher with 128 + N', posixOnly, () => {
	const piped = launch('kill -PIPE $$');
	assert.deepEqual([piped.status, piped.signal], [141, null]);
	const usr1 = launch('kill -USR1 $$');
	assert.deepEqual([usr1.status, usr1.signal], [138, null]);
	assert.doesNotMatch(usr1.stderr, /Debugger listening/);
});

test('stopping the launcher stops the binary', posixOnly, async () => {
	const root = makeFixture([HOST], { script: '#!/bin/sh\necho $$ > "$0.pid"\nexec sleep 30\n' });
	const pidFile = join(root, 'node_modules', '@svg-toolkit', `language-server-${HOST}`, 'bin', `${BIN}.pid`);
	const child = spawn(process.execPath, [facade(root)], { env, stdio: 'ignore' });
	const exited = new Promise((resolve) => child.on('exit', (code, signal) => resolve({ code, signal })));
	for (let tries = 0; !existsSync(pidFile) && tries < 200; tries++) await delay(25);
	const pid = Number(readFileSync(pidFile, 'utf8'));
	child.kill('SIGTERM');
	assert.deepEqual(await exited, { code: null, signal: 'SIGTERM' });
	for (let tries = 0; tries < 200; tries++) {
		try {
			process.kill(pid, 0);
		} catch {
			return;
		}
		await delay(25);
	}
	assert.fail(`binary ${pid} kept running after the launcher stopped`);
});

test('a launcher that finds no binary exits 1 with the reason', () => {
	const root = makeFixture([FOREIGN]);
	const result = spawnSync(process.execPath, [facade(root)], { env, encoding: 'utf8', timeout: 30_000 });
	assert.equal(result.status, 1);
	assert.match(result.stderr, /no prebuilt binary is published for/);
	assert.match(result.stderr, /svg-language-server: No prebuilt binary is published for/);
});
