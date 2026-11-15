import { expect, test } from 'bun:test';
import { versionEdits } from '../../scripts/release-version.ts';
import { carriers, workspaceVersion } from '../actions/version-check/carriers.mjs';

test('workflow preparation updates every version carrier without tags or commits', async () => {
	const edits = await versionEdits('0.5.0');
	const cargo = edits.get('Cargo.toml');
	if (cargo === undefined) throw new Error('Workspace version update is missing');
	expect(workspaceVersion(cargo)).toBe('0.5.0');
	for (const carrier of carriers) {
		const content = edits.get(carrier.file);
		if (content === undefined) continue;
		const matches = carrier.all ? [...content.matchAll(carrier.pattern)] : [content.match(carrier.pattern)];
		expect(matches.length).toBeGreaterThan(0);
		for (const match of matches) expect(match?.[1]).toBe('0.5.0');
	}
	await expect(versionEdits('0.5.0\nmalformed')).rejects.toThrow('Expected a semantic version');
});
