// @ts-nocheck Deno
import { BaselineBadge } from '#component/BaselineBadge.tsx';
import { BrowserSupport } from '#component/BrowserSupport.tsx';
import { DocsLinks } from '#component/DocsLinks.tsx';
import { attributeSearchTokens, type NamedAttributeEntry } from '#src/view.ts';

interface Props {
	rows: NamedAttributeEntry[];
}

function formatScope(elements: string[]): string {
	if (elements.includes('*')) return 'global';
	return elements.join(', ');
}

export function AttributesTable({ rows }: Props) {
	return (
		<div class='table-scroll'>
			<table>
				<thead>
					<tr>
						<th scope='col'>Name</th>
						<th scope='col' data-col-min-width='260'>Elements</th>
						<th scope='col'>Baseline</th>
						<th scope='col'>Desktop support</th>
						<th scope='col'>Docs</th>
					</tr>
				</thead>
				<tbody>
					{rows.map((entry) => (
						<tr data-search={attributeSearchTokens(entry)}>
							<th scope='row'>
								<code>{entry.name}</code>
							</th>
							<td class='scope-cell'>{formatScope(entry.elements)}</td>
							<td>
								<small>Observed-context summary</small>
								<BaselineBadge baseline={entry.baseline} discouraged={entry.discouraged} />
								<details>
									<summary>By element ({entry.coverage.baseline_known}/{entry.coverage.contexts} known Baselines)</summary>
									{Object.entries(entry.contexts).map(([key, facts]) => (
										<div>
											<strong>{key.startsWith('svg.global_attributes.') ? 'Global' : key.split('.')[2]}</strong>
											<BaselineBadge baseline={facts.baseline} discouraged={facts.discouraged} />
											<BrowserSupport support={facts.browser_support} baselineStatus={facts.baseline?.status} />
										</div>
									))}
									<p>
										{entry.coverage.baseline_unknown} unknown, {entry.coverage.baseline_missing} missing. Unlisted elements have no recorded context.
									</p>
								</details>
							</td>
							<td>
								<BrowserSupport support={entry.browser_support} baselineStatus={entry.baseline?.status} />
							</td>
							<td>
								<DocsLinks
									mdnUrl={entry.mdn_url}
									specUrls={entry.spec_url}
									deprecated={entry.deprecated}
								/>
							</td>
						</tr>
					))}
					<tr class='table-empty' hidden>
						<td colspan={5}>No matches.</td>
					</tr>
				</tbody>
			</table>
		</div>
	);
}
