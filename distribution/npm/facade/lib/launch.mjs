import { resolveBinary } from '#resolve';
import { spawnSync } from 'node:child_process';
import process from 'node:process';

const { argv, exit, stderr } = process;

/** @param {string} name */
export default function launch(name) {
	try {
		const result = spawnSync(resolveBinary(name), argv.slice(2), {
			stdio: 'inherit',
			windowsHide: false,
		});
		if (result.error) throw result.error;
		// Child died from a signal (SIGINT, SIGTERM, …).
		// Re-raise it on ourselves so the parent shell sees `WIFSIGNALED` / exit
		// code 128 + N instead of a generic 1 — `set -e`, trap handlers,
		// and Ctrl+C chaining all depend on this.
		if (result.signal) {
			process.removeAllListeners(result.signal);
			process.kill(process.pid, result.signal);
			return;
		}
		exit(result.status ?? 1);
	} catch (err) {
		stderr.write(`${name}: ${err instanceof Error ? err.message : String(err)}\n`);
		exit(1);
	}
}
