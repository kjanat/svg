use tower_lsp_server::ls_types::{FormattingOptions, Position, Range, TextEdit};

use crate::positions::{end_position_utf16, u32_from_usize};

const MAX_DIFF_CELLS: usize = 4_000_000;

pub fn apply_whitespace_options(text: String, options: &FormattingOptions) -> String {
    let mut text = if options.trim_trailing_whitespace == Some(true) {
        trim_trailing_whitespace(&text)
    } else {
        text
    };
    if options.trim_final_newlines == Some(true) {
        let content_end = text.trim_end_matches(['\n', '\r']).len();
        if content_end < text.len() {
            let newline = if text[content_end..].starts_with("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            text.truncate(content_end);
            text.push_str(newline);
        }
    }
    if options.insert_final_newline == Some(true) && !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn trim_trailing_whitespace(text: &str) -> String {
    text.split_inclusive('\n')
        .fold(String::with_capacity(text.len()), |mut out, line| {
            let (content, ending) = line
                .strip_suffix("\r\n")
                .map(|content| (content, "\r\n"))
                .or_else(|| line.strip_suffix('\n').map(|content| (content, "\n")))
                .unwrap_or((line, ""));
            out.push_str(content.trim_end_matches([' ', '\t']));
            out.push_str(ending);
            out
        })
}

pub fn text_edits(old: &str, new: &str, within: Option<Range>) -> Vec<TextEdit> {
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new.split_inclusive('\n').collect();
    line_hunks(&old_lines, &new_lines)
        .into_iter()
        .filter(|hunk| within.is_none_or(|range| hunk.touches(range)))
        .map(|hunk| {
            let start = Position::new(u32_from_usize(hunk.old.start), 0);
            let end = if hunk.old.end < old_lines.len() {
                Position::new(u32_from_usize(hunk.old.end), 0)
            } else {
                end_position_utf16(old)
            };
            TextEdit::new(Range::new(start, end), new_lines[hunk.new].concat())
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct Hunk {
    old: std::ops::Range<usize>,
    new: std::ops::Range<usize>,
}

impl Hunk {
    fn touches(&self, range: Range) -> bool {
        let first = range.start.line as usize;
        let last = range.end.line as usize;
        if self.old.is_empty() {
            return (first..=last.saturating_add(1)).contains(&self.old.start);
        }
        self.old.start <= last && first < self.old.end
    }
}

fn line_hunks(old: &[&str], new: &[&str]) -> Vec<Hunk> {
    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let old_middle = prefix..old.len() - suffix;
    let new_middle = prefix..new.len() - suffix;
    if old_middle.is_empty() && new_middle.is_empty() {
        return Vec::new();
    }
    let cells = old_middle.len().saturating_mul(new_middle.len());
    if old_middle.is_empty() || new_middle.is_empty() || cells > MAX_DIFF_CELLS {
        return vec![Hunk {
            old: old_middle,
            new: new_middle,
        }];
    }
    lcs_hunks(&old[old_middle], &new[new_middle])
        .into_iter()
        .map(|hunk| Hunk {
            old: hunk.old.start + prefix..hunk.old.end + prefix,
            new: hunk.new.start + prefix..hunk.new.end + prefix,
        })
        .collect()
}

fn lcs_hunks(old: &[&str], new: &[&str]) -> Vec<Hunk> {
    let width = new.len() + 1;
    let mut table = vec![0_u32; (old.len() + 1) * width];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            table[i * width + j] = if old[i] == new[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let mut hunks = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut open: Option<(usize, usize)> = None;
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            if let Some((old_start, new_start)) = open.take() {
                hunks.push(Hunk {
                    old: old_start..i,
                    new: new_start..j,
                });
            }
            i += 1;
            j += 1;
            continue;
        }
        open.get_or_insert((i, j));
        if j < new.len()
            && (i == old.len() || table[i * width + j + 1] >= table[(i + 1) * width + j])
        {
            j += 1;
        } else {
            i += 1;
        }
    }
    if let Some((old_start, new_start)) = open {
        hunks.push(Hunk {
            old: old_start..i,
            new: new_start..j,
        });
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(old: &str, edits: &[TextEdit]) -> String {
        let lines: Vec<&str> = old.split_inclusive('\n').collect();
        let mut out = String::new();
        let mut line = 0;
        for edit in edits {
            let start = edit.range.start.line as usize;
            out.push_str(&lines[line..start].concat());
            out.push_str(&edit.new_text);
            line = if edit.range.end.character == 0 {
                edit.range.end.line as usize
            } else {
                lines.len()
            };
        }
        out.push_str(&lines[line.min(lines.len())..].concat());
        out
    }

    fn options() -> FormattingOptions {
        FormattingOptions {
            tab_size: 2,
            insert_spaces: true,
            ..Default::default()
        }
    }

    #[test]
    fn whitespace_options_follow_the_client_request() {
        let text = "<svg>  \n\t<g/>\t\n</svg>\n\n\n".to_owned();
        assert_eq!(apply_whitespace_options(text.clone(), &options()), text);
        let trimmed = FormattingOptions {
            trim_trailing_whitespace: Some(true),
            trim_final_newlines: Some(true),
            ..options()
        };
        assert_eq!(
            apply_whitespace_options(text, &trimmed),
            "<svg>\n\t<g/>\n</svg>\n"
        );
        let final_newline = FormattingOptions {
            insert_final_newline: Some(true),
            ..options()
        };
        assert_eq!(
            apply_whitespace_options("<svg/>".to_owned(), &final_newline),
            "<svg/>\n"
        );
        assert_eq!(apply_whitespace_options(String::new(), &final_newline), "");
        let crlf = FormattingOptions {
            trim_trailing_whitespace: Some(true),
            trim_final_newlines: Some(true),
            ..options()
        };
        assert_eq!(
            apply_whitespace_options("<svg> \r\n</svg>\r\n\r\n".to_owned(), &crlf),
            "<svg>\r\n</svg>\r\n"
        );
    }

    #[test]
    fn edits_rewrite_only_changed_lines() {
        let old = "<svg>\n<g>\n<rect/>\n</g>\n<circle/>\n</svg>\n";
        let new = "<svg>\n  <g>\n    <rect/>\n  </g>\n<circle/>\n</svg>\n";
        let edits = text_edits(old, new, None);
        assert_eq!(apply(old, &edits), new);
        assert!(edits.iter().all(|edit| edit.range.start.line >= 1));
        assert!(edits.iter().all(|edit| edit.range.end.line <= 4));
    }

    #[test]
    fn range_keeps_only_the_hunks_it_touches() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "A\nb\nc\nd\nE\n";
        let edits = text_edits(
            old,
            new,
            Some(Range::new(Position::new(4, 0), Position::new(4, 1))),
        );
        assert_eq!(edits.len(), 1);
        assert_eq!(apply(old, &edits), "a\nb\nc\nd\nE\n");
        assert_eq!(text_edits(old, old, None), vec![]);
    }

    #[test]
    fn edits_reach_the_end_without_a_final_newline() {
        let old = "<svg>\n<g/></svg>";
        let new = "<svg>\n  <g/>\n</svg>\n";
        let edits = text_edits(old, new, None);
        assert_eq!(
            edits,
            vec![TextEdit::new(
                Range::new(Position::new(1, 0), Position::new(1, 10)),
                "  <g/>\n</svg>\n".to_owned()
            )]
        );
    }

    #[test]
    fn insertions_and_deletions_round_trip() {
        for (old, new) in [
            ("a\nb\n", "a\nx\nb\n"),
            ("a\nx\nb\n", "a\nb\n"),
            ("", "a\n"),
            ("a\n", ""),
            ("a\nb\nc\n", "c\nb\na\n"),
        ] {
            assert_eq!(
                apply(old, &text_edits(old, new, None)),
                new,
                "{old:?} -> {new:?}"
            );
        }
    }
}
