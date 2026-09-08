# Complete browser facts and configurable hover

Store all BCD products and support history so presentation choices do not limit
future uses of the data. Keep Web Features' independently resolved support map.

The Rust generator, build script and runtime use shared generic support and flag
types. Static strings and slices compile into the catalog; owned collections
hold refreshed data. JSON maps product IDs to complete statement arrays. Field
names and version/flag unions follow BCD; singleton note/link fields normalize
to arrays. Selecting a current implementation is a presentation operation.

`svg.hover` selects browsers, sections, detail fields and whether to show full
history. Defaults keep the four desktop products and compact details. Settings
apply during initialization and configuration notifications. Invalid settings
retain the previous valid preferences and produce a warning. These preferences
do not filter source data or alter diagnostic policy.

Custom templates, user HTML and browser-targeted diagnostics are outside scope.

Verify complete statement retention in a shared fixture, equality of static and
runtime facts, exact Web Features overrides, real protocol settings changes,
worker schema validity and the repository's full `just verify` checks.
