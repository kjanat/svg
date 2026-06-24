export const D_ATTRIBUTE_NAMES: readonly string[] = ['d', 'path'];

/**
 * Attribute names handled by dedicated grammar rules instead of generated
 * `ATTRIBUTE_BUCKETS` lists. Keep aligned with `svg-data-regen`.
 */
export const GRAMMAR_DEDICATED_ATTRIBUTE_NAMES: readonly string[] = [
	'class',
	'clip',
	'd',
	'dur',
	'enable-background',
	'gradientTransform',
	'href',
	'id',
	'keySplines',
	'keyTimes',
	'offset',
	'path',
	'patternTransform',
	'preserveAspectRatio',
	'repeatCount',
	'repeatDur',
	'style',
	'transform',
	'xlink:href',
];

export const PATH_COMMAND_TOKEN_RULES: readonly string[] = [
	'moveto_command',
	'closepath_command',
	'lineto_command',
	'horizontal_lineto_command',
	'vertical_lineto_command',
	'curveto_command',
	'smooth_curveto_command',
	'quadratic_bezier_curveto_command',
	'smooth_quadratic_bezier_curveto_command',
	'elliptical_arc_command',
];

/** Attribute buckets consumed via `choice(...ATTRIBUTE_BUCKETS.*)` in grammar.js. */
export const GENERATED_ATTRIBUTE_BUCKET_KEYS: readonly string[] = [
	'keyword',
	'color',
	'length',
	'length_list',
	'length_list_or_none',
	'number',
	'number_list',
	'number_optional_number',
	'number_or_percentage',
	'coordinate_pair_list',
	'view_box',
	'functional_iri',
	'css_text',
];

/** Token sets consumed via `TOKENS.*` in grammar.js. */
export const TOKEN_KEYS: readonly string[] = [
	'length_units',
	'angle_units',
	'time_units',
	'color_spaces',
	'color_interpolation_spaces',
	'hue_interpolation_methods',
];
