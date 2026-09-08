export function CompatibilityGuide() {
	return (
		<details class='compatibility-guide' id='compatibility-guide'>
			<summary>How to read Baseline and browser support</summary>
			<p>
				<strong>Newly Available</strong> means a feature has reached WebDX's core browser set under its policy. <strong>Widely Available</strong>{' '}
				additionally meets the upstream criteria for longer-established availability. <strong>Limited availability</strong>{' '}
				means it is not designated Baseline; this can reflect WebDX discouragement as well as browser support gaps. Missing or unrecognized data is
				shown as <strong>Unknown</strong>, without a Baseline badge.
			</p>
			<p>
				The optional Newly and Widely dates are separate milestones. For example, a feature can become Newly in 2020 and Widely in 2022. Hover a
				status for both recorded dates and any qualifiers. A recognized status can have no date. Discouragement replaces the icon with the reason,
				references, and alternatives.
			</p>
			<p>
				Baseline comes from <code>web-features</code>; browser details come from MDN{' '}
				<code>@mdn/browser-compat-data</code>. The source table identifies the package versions used for this response. Defaults are pinned; the
				version controls request other releases. Both sources must load successfully; a failed request returns an error instead of silently
				substituting older facts.
			</p>
			<p>
				Desktop support shows Chrome, Edge, Firefox, and Safari, with details on hover. Baseline's core set also includes Chrome and Firefox on
				Android and Safari on iOS. These are browser products and platforms. Baseline does not guarantee support in every browser, WebView,
				non-browser SVG renderer, or your application's deployment environment.
			</p>
			<p>
				Attribute rows are <strong>project-derived summaries</strong>{' '}
				of observed contexts: the least favorable known tier, with the later matching milestone on ties. Unknown and missing coverage remains
				separate. Expand “By element” for the actual contextual facts; the summary does not apply to every use of an attribute. The editor's
				Caution/Avoid advice is svg policy, separate from upstream Baseline and SVG specification validity.
			</p>
			<p>
				<a href='https://github.com/kjanat/svg/blob/master/docs/baseline.md'>Compatibility guide and editor offline behavior</a>
				{' · '}
				<a href='https://github.com/web-platform-dx/web-features/blob/main/docs/baseline.md'>Upstream Baseline definition</a>
			</p>
			<p class='muted'>
				Baseline artwork © Google LLC, licensed under{' '}
				<a href='https://creativecommons.org/licenses/by-nd/4.0/'>CC BY-ND 4.0</a>. Baseline and its logos are Google trademarks. Unmodified official
				light/dark assets; <a href='https://web-platform-dx.github.io/name-and-logo-usage-guidelines/'>source and usage guidelines</a>.
			</p>
		</details>
	);
}
