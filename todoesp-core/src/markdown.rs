//! Minimal Markdown stripping used to render Todoist task content as plain text.

use alloc::format;
use alloc::string::String;

/// Strip markdown control sequences from a string, returning the plain text
/// representation.
///
/// This function only supports trivial Markdown control sequences, such as:
/// - `**bold**`
/// - `*italic*`
/// - `~strikethrough~`
/// - `` `code` ``
/// - `[link](https://example.com)`
/// - `![image](https://example.com/image.png)`
/// - `# Header`
pub fn strip(src: &str, max_len: usize) -> String {
    if src.is_empty() {
        return String::new();
    }

    let src = src.trim_start_matches("# ");

    let mut result = String::new();
    let mut i = 0;
    while i < src.len() && result.len() < max_len {
        match (
            src.chars().nth(i).unwrap_or_default(),
            src.chars().nth(i + 1),
        ) {
            (c, Some(c2)) if c == c2 && (c == '*' || c == '_' || c == '~') => {
                i += 2;
                let end = src[i..]
                    .find(&format!("{}{}", c, c2))
                    .unwrap_or(src.len() - i);
                result.push_str(&src[i..i + end]);
                i += end + 2;
            }
            (c, _) if c == '_' || c == '~' || c == '*' || c == '`' => {
                i += 1;
                let end = src[i..].find(c).unwrap_or(src.len() - i);
                result.push_str(&src[i..i + end]);
                i += end + 1;
            }
            ('[', _) => {
                i += 1;
                let end = src[i..].find(']').unwrap_or(src.len() - i);
                result.push_str(&strip(&src[i..i + end], max_len - result.len()));
                i += end + 1;

                let end = src[i..].find(')').unwrap_or(src.len() - i);
                i += end + 1;
            }
            ('!', Some('[')) => {
                i += 2;
                let end = src[i..].find(']').unwrap_or(src.len() - i);
                result.push_str(&strip(&src[i..i + end], max_len - result.len()));
                i += end + 1;

                let end = src[i..].find(')').unwrap_or(src.len() - i);
                i += end + 1;
            }
            (c, _) => {
                result.push(c);
                i += 1;
            }
        }
    }

    if result.len() > max_len {
        result.truncate(max_len - 3);
        result.push_str("...");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::strip;

    #[test]
    fn passes_through_plain_text() {
        assert_eq!(strip("hello world", 80), "hello world");
    }

    #[test]
    fn empty_string() {
        assert_eq!(strip("", 80), "");
    }

    #[test]
    fn strips_bold_and_italic() {
        assert_eq!(strip("**bold**", 80), "bold");
        assert_eq!(strip("*italic*", 80), "italic");
        assert_eq!(strip("__bold__", 80), "bold");
        assert_eq!(strip("_italic_", 80), "italic");
    }

    #[test]
    fn strips_strikethrough_and_code() {
        assert_eq!(strip("~struck~", 80), "struck");
        assert_eq!(strip("`code`", 80), "code");
    }

    #[test]
    fn strips_leading_header_marker() {
        assert_eq!(strip("# Heading", 80), "Heading");
    }

    #[test]
    fn strips_link_keeping_label() {
        assert_eq!(strip("[label](https://example.com)", 80), "label");
    }

    #[test]
    fn strips_image_keeping_alt_text() {
        assert_eq!(strip("![alt](https://example.com/image.png)", 80), "alt");
    }

    #[test]
    fn stops_at_max_len_for_plain_text() {
        // Plain text is appended one character at a time and simply stops once
        // the limit is reached (no ellipsis is added in this path).
        let result = strip("abcdefghij", 6);
        assert_eq!(result, "abcdef");
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn truncates_with_ellipsis_when_segment_overshoots() {
        // A formatting segment is appended in bulk, overshooting the limit, so
        // the result is truncated and an ellipsis appended.
        let result = strip("**abcdefghij**", 6);
        assert_eq!(result, "abc...");
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn mixed_formatting() {
        assert_eq!(
            strip("Read **the** [docs](https://example.com) `now`", 80),
            "Read the docs now"
        );
    }
}
