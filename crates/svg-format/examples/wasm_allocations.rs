//! Runtime regression for the C allocator linked into wasm32-unknown-unknown.

fn main() {
    // Keep a Rust allocation alive across C parser/scanner allocations.
    let sentinel = vec![0xa5_u8; 2 * 1024 * 1024];
    let source = format!("<svg>{}</svg>", "<rect/>".repeat(22_204));
    let expected = format!("<svg>\n{}</svg>", "  <rect />\n".repeat(22_204));
    for _ in 0..3 {
        let formatted = svg_format::format(&source);
        assert_eq!(formatted, expected);
        assert_eq!(svg_format::format(&formatted), formatted);
        assert!(sentinel.iter().all(|byte| *byte == 0xa5));
    }
}
