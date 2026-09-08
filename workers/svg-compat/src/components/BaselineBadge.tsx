import type { Baseline, BaselineDate, Discouraged } from '#lib/types.ts';

interface Props {
	baseline: Baseline | undefined;
	discouraged?: Discouraged[];
}

const BADGE_SRC = {
	widely: '/badges/baseline-widely.svg',
	newly: '/badges/baseline-newly.svg',
	limited: '/badges/baseline-limited.svg',
} as const;
const QUALIFIER_GLYPH = { before: '≤', after: '≥', approximately: '~' };

function milestone(label: string, date: BaselineDate | undefined): string | undefined {
	if (!date) return undefined;
	return date.date ? `${label} since ${date.raw}` : `${label} date not recognized (raw: ${date.raw})`;
}

function baselineTitle(baseline: Baseline): string {
	const parts = [milestone('Newly Available', baseline.low_date), milestone('Widely Available', baseline.high_date)];
	if (!baseline.status) {
		parts.unshift(`No recognized Baseline status (${baseline.status_diagnostic ?? 'no data'}; raw: ${baseline.raw_status ?? 'absent'})`);
	} else if (baseline.status === 'limited') {
		parts.unshift('Not designated Baseline; this does not necessarily mean a missing browser implementation.');
	}
	return parts.filter(Boolean).join(' · ');
}

function referenceUrl(raw: string): string | undefined {
	try {
		const url = new URL(raw);
		return url.protocol === 'https:' || url.protocol === 'http:' ? url.href : undefined;
	} catch {
		return undefined;
	}
}

export function BaselineBadge({ baseline, discouraged }: Props) {
	if (discouraged?.length) {
		return (
			<div class='webdx-discouraged'>
				{discouraged.map(advice => (
					<details key={`${advice.feature_id}:${advice.compat_key}`}>
						<summary>WebDX discourages {advice.feature_name ?? advice.feature_id}</summary>
						<p>{advice.reason}</p>
						<p class='muted'>Feature: {advice.feature_id} · Context: {advice.compat_key}</p>
						{advice.according_to.map(url => <p key={url}>{referenceUrl(url) ? <a href={referenceUrl(url)}>Supporting reference</a> : url}</p>)}
						{advice.alternatives.length > 0 && <p>Alternatives: {advice.alternatives.join(', ')}</p>}
						{advice.removal_date && <p>Upstream removal date: {advice.removal_date}</p>}
					</details>
				))}
			</div>
		);
	}
	if (!baseline?.status) return <span class='muted' title={baseline ? baselineTitle(baseline) : 'No Baseline data'}>Unknown</span>;
	const variant = baseline.status;
	const date = variant === 'widely' ? baseline.high_date : variant === 'newly' ? baseline.low_date : undefined;
	const year = date?.date?.slice(0, 4);
	const glyph = date?.qualifier ? QUALIFIER_GLYPH[date.qualifier] : '';
	const label = variant === 'widely' ? 'Widely Available' : variant === 'newly' ? 'Newly Available' : 'Limited availability';
	return (
		<span class={`badge badge-${variant}`} title={baselineTitle(baseline)}>
			<img class='badge-icon' src={BADGE_SRC[variant]} alt='' width='18' height='10' />
			{label}
			{year ? ` since ${glyph}${year}` : ''}
		</span>
	);
}
