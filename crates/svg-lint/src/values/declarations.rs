//! CSS declaration lists (`style`).
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `CssDeclarationList`: a CSS declaration list.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_css_declaration_list(value: &str) -> Option<String> {
    let _ = value;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn valid(value: &str) {
        assert_eq!(
            check_css_declaration_list(value),
            None,
            "expected {value:?} to be valid"
        );
    }

    #[track_caller]
    fn invalid(value: &str) {
        assert!(
            check_css_declaration_list(value).is_some(),
            "expected {value:?} to be invalid"
        );
    }

    // Claim: "The ‘style’ attribute is used to supply a CSS declaration of
    // an element. The attribute is parsed as a declaration-list."
    // https://svgwg.org/svg2-draft/styling.html#StyleAttribute
    // Claim: "The value of the style attribute must match the syntax of the
    // contents of a CSS declaration block (excluding the delimiting braces)"
    // / "style-attribute : S* declaration-list ; declaration-list :
    // declaration [ ';' S* declaration-list ]? | at-rule declaration-list |
    // /* empty */ ;"
    // https://drafts.csswg.org/css-style-attr/#syntax
    #[test]
    fn real_world_declaration_lists() {
        valid("fill:none;stroke:#000000;stroke-width:2;marker-end:url(#Arrow2Mend)");
        valid(
            "font-size:20px;letter-spacing:0px;word-spacing:0px;text-anchor:middle;fill:#000000;\
             font-family:Nimbus Roman No9 L;-inkscape-font-specification:Nimbus Roman No9 L",
        );
        valid("overflow:visible;");
        valid("marker-end: url(#Arrow)");
        valid("stroke-dasharray: 50 50");
        valid("color: #090; line-height: 1.2");
        valid("fill:url(\"#grad\") red; font-family: 'Open Sans', serif");
        // Whitespace around and between declarations, and any case.
        valid("  FILL : RED ;  Stroke:Blue  ");
        valid("\n\tfill: red;\n\tstroke: blue;\n");
        // Comments are not tokens.
        valid("/* a */ fill /* b */ : /* c */ red /* d */ ; /* e */");
        // Escaped and non-ASCII property names are identifiers.
        valid("\\66 ill: red");
        valid("_height: 1px; -webkit-mask: none; prop\u{e9}t\u{e9}: x");
        // Unknown properties and values are valid syntax.
        valid("not-a-property: whatever 1 2 3 #fff");
        valid("fill: calc((1px + 2%) * var(--k, 3)) env(x) attr(y)");
        valid("fill: inherit; stroke: revert-layer");
    }

    // Claim: "declaration-list : declaration [ ';' S* declaration-list ]? |
    // at-rule declaration-list | /* empty */ ;"
    // https://drafts.csswg.org/css-style-attr/#syntax
    // Claim: "Discard whitespace from input. If the next token from input is
    // a <semicolon-token>, discard a token from input."
    // https://drafts.csswg.org/css-syntax-3/#parse-list-of-declarations
    #[test]
    fn empty_lists_and_stray_semicolons() {
        valid("");
        valid("   ");
        valid(";");
        valid(";;fill:red;;;stroke:blue;;");
    }

    // Claim: "Although the grammar allows it, no at-rule valid in style
    // attributes is define at the moment. The forward-compatible parsing
    // rules are such that a declaration following an at-rule is not
    // ignored: <span style="@unsupported { splines: reticulating } color:
    // green">"
    // https://drafts.csswg.org/css-style-attr/#syntax
    #[test]
    fn at_rules() {
        valid("@unsupported { splines: reticulating } color: green");
        valid("@foo bar baz; fill: red");
        valid("fill: red; @foo (x) [y]");
        valid("@foo { a { b: c } ; d } fill: red");
        valid("@foo ! : ;");
    }

    // Claim: "declaration : property S* ':' S* value;" / "property :
    // IDENT;"
    // https://www.w3.org/TR/CSS2/syndata.html#tokenization
    // Claim: "If the next token is an <ident-token>, consume a token from
    // input and set decl's name to the token’s value. Otherwise, consume
    // the remnants of a bad declaration from input, with nested, and return
    // nothing."
    // https://drafts.csswg.org/css-syntax-3/#consume-declaration
    #[test]
    fn declaration_starts_with_a_property_name() {
        valid("fill: red");
        valid("--x: 1");
        // An empty property name.
        invalid(": red");
        invalid("fill: red; : blue");
        invalid("fill: red;:blue");
        // Something other than an identifier where the name belongs.
        invalid("*zoom: 1");
        invalid("123: x");
        invalid("\"fill\": red");
        invalid("#fill: red");
        invalid("{fill: red}");
        invalid("fill: red; (x): y");
        invalid("f(x): y");
        invalid("<!-- fill: red");
    }

    // Claim: "declaration : property S* ':' S* value;"
    // https://www.w3.org/TR/CSS2/syndata.html#tokenization
    // Claim: "If the next token is a <colon-token>, discard a token from
    // input. Otherwise, consume the remnants of a bad declaration from
    // input, with nested, and return nothing."
    // https://drafts.csswg.org/css-syntax-3/#consume-declaration
    #[test]
    fn declaration_needs_a_colon() {
        valid("fill :red");
        valid("fill/**/:red");
        invalid("fill");
        invalid("fill red");
        invalid("fill = red");
        invalid("fill: red; stroke");
        invalid("fill: red stroke: blue; x");
        invalid("fill;stroke:red");
        invalid("fill (x): red");
        invalid("--x: a; b");
    }

    // Claim: "Consume a list of component values from input, with nested,
    // and with <semicolon-token> as the stop token, and set decl’s value to
    // the result." / "If decl’s name is a custom property name string, then
    // set decl’s original text to the segment of the original source text
    // string corresponding to the tokens of decl’s value."
    // https://drafts.csswg.org/css-syntax-3/#consume-declaration
    // Claim: "The <declaration-value> production matches any sequence of one
    // or more tokens, so long as the sequence does not contain
    // <bad-string-token>, <bad-url-token>, unmatched <)-token>, <]-token>,
    // or <}-token>, or top-level <semicolon-token> tokens or <delim-token>
    // tokens with a value of "!"."
    // https://drafts.csswg.org/css-syntax-3/#typedef-declaration-value
    #[test]
    fn custom_properties() {
        valid("--x: anything at all 1px #abc");
        valid("--x:");
        valid("--x: ;fill:red");
        valid("--x: {a; b: c} ; --y: [;] (;) f(;)");
        valid("--x: :::; --y: @media");
        valid("--\u{1f600}: 1");
        invalid("--x: a)");
        invalid("--x: {a]}");
    }

    // Claim: "If the last two non-<whitespace-token>s in decl’s value are a
    // <delim-token> with the value "!" followed by an <ident-token> with a
    // value that is an ASCII case-insensitive match for "important", remove
    // them from decl’s value and set decl’s important flag."
    // https://drafts.csswg.org/css-syntax-3/#consume-declaration
    // Claim: "The <declaration-value> production matches any sequence of one
    // or more tokens, so long as the sequence does not contain [...]
    // top-level <semicolon-token> tokens or <delim-token> tokens with a
    // value of "!". It represents the entirety of what a valid declaration
    // can have as its value."
    // https://drafts.csswg.org/css-syntax-3/#typedef-declaration-value
    #[test]
    fn important_and_bang() {
        valid("fill: red !important");
        valid("fill: red!important;stroke:blue ! IMPORTANT ");
        valid("fill: red !/* c */important");
        valid("fill: red !\\69mportant");
        valid("--x: a !important");
        valid("--x:!important");
        // A `!` inside a function or block is not top-level.
        valid("--x: [!] (!) {!} f(!)");
        valid("content: \"!\"; fill: url(!)");
        invalid("fill: red !ie");
        invalid("fill: red ! foo");
        invalid("fill: !important red");
        invalid("fill: red !important !important");
        invalid("fill: red !");
        invalid("--x: a ! b");
        invalid("fill: red !important; stroke: blue !");
    }

    // Claim: "The <any-value> production is identical to
    // <declaration-value>, but also allows top-level <semicolon-token>
    // tokens and <delim-token> tokens with a value of "!". It represents
    // the entirety of what valid CSS can be in any context."
    // https://drafts.csswg.org/css-syntax-3/#typedef-any-value
    // Claim: "The <declaration-value> production matches any sequence of one
    // or more tokens, so long as the sequence does not contain
    // <bad-string-token>, <bad-url-token>, unmatched <)-token>, <]-token>,
    // or <}-token>"
    // https://drafts.csswg.org/css-syntax-3/#typedef-declaration-value
    #[test]
    fn unmatched_closing_brackets() {
        valid("fill: rgb(1 2 3); stroke: [a] {b} (c)");
        valid("fill: f(g(h[i{j}]))");
        invalid("fill: red)");
        invalid("fill: red]");
        invalid(")fill: red");
        invalid("fill: (a]");
        invalid("fill: [a)]");
        invalid("fill: f(a})");
        invalid("@foo ) ; fill: red");
        invalid("@foo { a: ( ] ) } fill: red");
    }

    // Claim: "<}-token> If nested is true, return values. Otherwise, this is
    // a parse error. Consume a token from input and append the result to
    // values."
    // https://drafts.csswg.org/css-syntax-3/#consume-list-of-components
    // Claim: "Note that because there is no open brace delimiting the
    // declaration list in the CSS style attribute syntax, a close brace (})
    // in the style attribute's value does not terminate the style data: it
    // is merely an invalid token."
    // https://drafts.csswg.org/css-style-attr/#syntax
    #[test]
    fn stray_close_brace() {
        valid("fill: {a}");
        invalid("fill: red }");
        invalid("fill: red; }");
        invalid("} fill: red");
        invalid("fill: red } stroke: blue");
    }

    // Claim: "newline This is a parse error. Reconsume the current input
    // code point, create a <bad-string-token>, and return it."
    // https://drafts.csswg.org/css-syntax-3/#consume-string-token
    // Claim: "Conformance checkers must report at least one parse error
    // condition to the user if one or more parse error conditions exist in
    // the document and must not report parse error conditions if none exist
    // in the document."
    // https://drafts.csswg.org/css-syntax-3/#parse-error
    // Claim: "For a white space character (#x20, #xD, #xA, #x9), append a
    // space character (#x20) to the normalized value."
    // https://www.w3.org/TR/xml/#AVNormalize
    #[test]
    fn bad_strings() {
        valid("content: 'a\\\u{c}b'");
        valid("content: \"a;b\" 'c\"d' \"e\\\"f\"");
        // A literal line break in an attribute value reaches CSS as a space
        // after XML attribute-value normalization; the check cannot tell it
        // from a character reference, so it never reads it as a newline.
        valid("font-family: 'Open\nSans'");
        valid("font-family: 'Open\r\nSans'");
        // U+000C is a CSS newline and not XML whitespace.
        invalid("font-family: 'Open\u{c}Sans'");
        invalid("content: \"a\u{c}\"; fill: red");
    }

    // Claim: "U+0022 QUOTATION MARK (") U+0027 APOSTROPHE (') U+0028 LEFT
    // PARENTHESIS (() non-printable code point This is a parse error.
    // Consume the remnants of a bad url, create a <bad-url-token>, and
    // return it."
    // https://drafts.csswg.org/css-syntax-3/#consume-url-token
    // Claim: "<bad-url-token>" is excluded from "<declaration-value>"
    // https://drafts.csswg.org/css-syntax-3/#typedef-declaration-value
    #[test]
    fn bad_urls() {
        valid("fill: url(#a) red");
        valid("fill: url(  #a  )");
        valid("fill: url( \"#a b\" )");
        valid("fill: url('#a(')");
        valid("fill: url(a\\)b\\ c)");
        invalid("fill: url(#a b)");
        invalid("fill: url(#a\"b)");
        invalid("fill: url(#a(b)");
        invalid("fill: url(#a\u{1}b)");
    }

    // Claim: "EOF This is a parse error. Return the <string-token>."
    // https://drafts.csswg.org/css-syntax-3/#consume-string-token
    // Claim: "Conformance checkers must report at least one parse error
    // condition to the user if one or more parse error conditions exist in
    // the document"
    // https://drafts.csswg.org/css-syntax-3/#parse-error
    #[test]
    fn unterminated_strings() {
        valid("font-family: 'Open Sans'");
        valid("content: \"\"");
        valid("content: \"\\\\\"");
        valid("content: 'a' /* trailing comment */");
        invalid("font-family: 'Open Sans");
        invalid("font-family: 'Open Sans;fill:red");
        invalid("content: \"");
        invalid("content: \"abc\\\"");
        invalid("content: \"abc\\");
        invalid("content: 'abc\\\\\\'");
    }

    // Claim: "EOF This is a parse error. Return the <url-token>." / "If the
    // next input code point is U+0029 RIGHT PARENTHESIS ()) or EOF, consume
    // it and return the <url-token> (if EOF was encountered, this is a parse
    // error)"
    // https://drafts.csswg.org/css-syntax-3/#consume-url-token
    #[test]
    fn unterminated_urls() {
        valid("fill: url(#a)");
        valid("fill: url(#a) /* x */");
        invalid("fill: url(#a");
        invalid("fill: url(#a  ");
        invalid("fill: url(");
        invalid("fill: url(#a\\)");
    }

    // Claim: "If the preceding paragraph ended by consuming an EOF code
    // point, this is a parse error."
    // https://drafts.csswg.org/css-syntax-3/#consume-comment
    #[test]
    fn unterminated_comments() {
        valid("fill: red /* a */");
        valid("fill: red /**/ /***/");
        valid("content: '/*'");
        invalid("fill: red /* a");
        invalid("fill: red /* a */ /*");
        invalid("/* fill: red");
        invalid("fill: red /* a *");
    }

    // Claim: "U+005C REVERSE SOLIDUS (\) If the input stream starts with a
    // valid escape, reconsume the current input code point, consume an
    // ident-like token, and return it. Otherwise, this is a parse error.
    // Return a <delim-token> with its value set to the current input code
    // point."
    // https://drafts.csswg.org/css-syntax-3/#consume-token
    // Claim: "EOF This is a parse error. Return U+FFFD REPLACEMENT CHARACTER
    // (�)."
    // https://drafts.csswg.org/css-syntax-3/#consume-escaped-code-point
    #[test]
    fn invalid_escapes() {
        valid("fill: r\\65 d");
        valid("fill: a\\\\");
        // A literal line break after `\` reaches CSS as an escaped space.
        valid("fill: a\\\nb");
        invalid("fill: a\\\u{c}b");
        invalid("fill: red\\");
        invalid("fill: #ab\\");
        invalid("fill: 1px\\");
        invalid("--x\\");
    }

    // Claim: "<EOF-token> ending token Discard a token from input. Return
    // block."
    // https://drafts.csswg.org/css-syntax-3/#consume-simple-block
    // Claim: "<eof-token> <)-token> Discard a token from input. Return
    // function."
    // https://drafts.csswg.org/css-syntax-3/#consume-function
    // (Blocks and functions left open at the end of the value are closed
    // without a parse error.)
    #[test]
    fn open_blocks_at_end_are_accepted() {
        valid("fill: rgb(1 2 3");
        valid("--x: [a");
        valid("--x: {a; b");
        valid("fill: calc((1 + 2)");
        valid("@foo { a: b");
    }
}
