# svg-path-preview

SVG path parsing, geometry bounds, and braille and SVG previews, independent of
any editor or language-server protocol. Geometry comes from the workspace's
`tree-sitter-svg-path` grammar; the preview retains the original SVG commands.

`sketch` accepts path data. `sketch_for_attribute` accepts a parsed SVG
path-data attribute and decodes XML character references before rendering. Both
return `None` for unsupported, invalid, oversized, or non-drawing input.
