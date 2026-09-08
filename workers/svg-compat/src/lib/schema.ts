/**
 * JSON Schema (2020-12) describing the `/data.json` response shape.
 * Served verbatim at `/schema.json` and emitted by the CLI's
 * `emit schema` subcommand.
 *
 * Kept in its own module so the schema constant can be imported
 * without dragging in any of the parse / build runtime.
 *
 * @module
 */

const COMPAT_ENTRY_SCHEMA = {
	type: 'object',
	required: ['deprecated', 'experimental', 'standard_track', 'spec_url'],
	additionalProperties: false,
	properties: {
		description: { type: 'string' },
		mdn_url: { type: 'string', format: 'uri' },
		deprecated: { type: 'boolean' },
		experimental: { type: 'boolean' },
		standard_track: { type: 'boolean' },
		spec_url: {
			type: 'array',
			items: { type: 'string', format: 'uri' },
		},
		baseline: { $ref: '#/$defs/baseline' },
		discouraged: { type: 'array', items: { $ref: '#/$defs/discouraged' } },
		browser_support: { $ref: '#/$defs/browserSupport' },
	},
} as const;

export const SVG_COMPAT_SCHEMA = {
	$schema: 'https://json-schema.org/draft/2020-12/schema',
	title: 'SVG Compat Output v2',
	type: 'object',
	required: ['schema_version', 'generated_at', 'sources', 'elements', 'attributes'],
	additionalProperties: false,
	properties: {
		schema_version: { const: 2 },
		generated_at: { type: 'string', format: 'date-time' },
		sources: {
			type: 'object',
			required: ['bcd', 'web_features'],
			additionalProperties: false,
			properties: {
				bcd: { $ref: '#/$defs/sourceInfo' },
				web_features: { $ref: '#/$defs/sourceInfo' },
			},
		},
		elements: {
			type: 'object',
			additionalProperties: { $ref: '#/$defs/compatEntry' },
		},
		attributes: {
			type: 'object',
			additionalProperties: { $ref: '#/$defs/attributeEntry' },
		},
	},
	$defs: {
		sourceInfo: {
			type: 'object',
			required: ['package', 'requested', 'resolved', 'mode', 'source_url'],
			additionalProperties: false,
			properties: {
				package: { type: 'string' },
				requested: { type: 'string' },
				resolved: { type: 'string' },
				mode: { enum: ['default', 'override'] },
				source_url: { type: 'string', format: 'uri' },
			},
		},
		baseline: {
			type: 'object',
			required: [],
			additionalProperties: false,
			properties: {
				status: { enum: ['limited', 'newly', 'widely'] },
				status_diagnostic: { enum: ['missing', 'unrecognized'] },
				/* Original upstream baseline field encoded as JSON. */
				raw_status: { type: 'string' },
				support: { type: 'object', additionalProperties: { type: 'string' } },
				low_date: { $ref: '#/$defs/baselineDate' },
				high_date: { $ref: '#/$defs/baselineDate' },
			},
		},
		discouraged: {
			type: 'object',
			required: ['feature_id', 'compat_key', 'scope', 'reason', 'according_to', 'alternatives'],
			additionalProperties: false,
			properties: {
				feature_id: { type: 'string' },
				compat_key: { type: 'string' },
				scope: { const: 'feature' },
				feature_name: { type: 'string' },
				reason: { type: 'string' },
				reason_html: { type: 'string' },
				according_to: { type: 'array', items: { type: 'string' } },
				alternatives: { type: 'array', items: { type: 'string' } },
				removal_date: { type: 'string' },
			},
		},
		baselineDate: {
			type: 'object',
			required: ['raw'],
			additionalProperties: false,
			properties: {
				/* Original upstream value, byte-for-byte. Always present. */
				raw: { type: 'string' },
				/* ISO YYYY-MM-DD extracted from `raw`. Absent if unparseable. */
				date: { type: 'string', format: 'date' },
				qualifier: { enum: ['before', 'after', 'approximately'] },
			},
		},
		browserSupport: {
			type: 'object',
			additionalProperties: { type: 'array', items: { $ref: '#/$defs/browserVersion' } },
		},
		browserVersion: {
			type: 'object',
			additionalProperties: false,
			required: ['flags', 'notes', 'impl_url', 'partial_implementation'],
			properties: {
				version_added: { anyOf: [{ type: 'string' }, { const: false }] },
				version_removed: { type: 'string' },
				version_last: { type: 'string' },
				partial_implementation: { type: 'boolean' },
				prefix: { type: 'string' },
				alternative_name: { type: 'string' },
				flags: { type: 'array', items: { $ref: '#/$defs/browserFlag' } },
				notes: { type: 'array', items: { type: 'string' } },
				impl_url: { type: 'array', items: { type: 'string' } },
			},
		},
		browserFlag: {
			type: 'object',
			required: ['type', 'name'],
			additionalProperties: false,
			properties: { type: { enum: ['preference', 'runtime_flag'] }, name: { type: 'string' }, value_to_set: { type: 'string' } },
		},
		compatEntry: COMPAT_ENTRY_SCHEMA,
		attributeEntry: {
			type: 'object',
			additionalProperties: false,
			required: ['elements', 'contexts', 'aggregate', 'aggregation', 'coverage'],
			properties: {
				elements: { type: 'array', items: { type: 'string' } },
				contexts: { type: 'object', additionalProperties: { $ref: '#/$defs/compatEntry' } },
				aggregate: { $ref: '#/$defs/compatEntry' },
				aggregation: { const: 'observed-contexts' },
				coverage: {
					type: 'object',
					additionalProperties: false,
					required: ['contexts', 'baseline_known', 'baseline_unknown', 'baseline_missing'],
					properties: {
						contexts: { type: 'integer', minimum: 0 },
						baseline_known: { type: 'integer', minimum: 0 },
						baseline_unknown: { type: 'integer', minimum: 0 },
						baseline_missing: { type: 'integer', minimum: 0 },
					},
				},
			},
		},
	},
} as const;
