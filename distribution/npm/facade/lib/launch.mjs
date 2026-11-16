import { resolveBinary } from '#resolve';
import { spawn } from 'node:child_process';
import { constants } from 'node:os';
import process from 'node:process';

const { argv, exit, stderr } = process;

/** Signals the launcher passes on, so stopping the launcher stops the binary. */
const FORWARDED = /** @type {const} */ (['SIGINT', 'SIGTERM', 'SIGHUP', 'SIGQUIT', 'SIGUSR2']);

/**
 * Signals Node ignores or claims for itself (SIGUSR1 starts the inspector),
 * so re-raising them cannot end the launcher with the child's status.
 */
const UNRAISABLE = new Set(['SIGPIPE', 'SIGUSR1']);

/**
 * @param {string} name
 * @param {unknown} err
 * @returns {never}
 */
function fail(name, err) {
	stderr.write(`${name}: ${err instanceof Error ? err.message : String(err)}\n`);
	exit(1);
}

/** @param {string} name */
export default function launch(name) {
	let binary;
	try {
		binary = resolveBinary(name);
	} catch (err) {
		fail(name, err);
	}

	const child = spawn(binary, argv.slice(2), { stdio: 'inherit', windowsHide: false });

	/** @param {NodeJS.Signals} signal */
	const forward = (signal) => {
		child.kill(signal);
	};
	if (process.platform !== 'win32') { for (const signal of FORWARDED) process.on(signal, forward); }

	child.on('error', (err) => fail(name, err));
	child.on('exit', (code, signal) => {
		for (const forwarded of FORWARDED) process.removeListener(forwarded, forward);
		// Child died from a signal (SIGINT, SIGTERM, …).
		// Re-raise it on ourselves so the parent shell sees `WIFSIGNALED` / exit
		// code 128 + N instead of a generic 1 — `set -e`, trap handlers,
		// and Ctrl+C chaining all depend on this. POSIX-only: Windows has no
		// signal exit semantics (signal is effectively never set there,
		// and re-raising would abort with an unrelated code), so fall through
		// to the generic failure exit instead.
		if (signal && process.platform !== 'win32') {
			if (!UNRAISABLE.has(signal)) {
				process.removeAllListeners(signal);
				process.kill(process.pid, signal);
			}
			exit(128 + (constants.signals[signal] ?? 0));
		}
		exit(code ?? 1);
	});
}
