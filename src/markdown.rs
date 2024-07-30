/// Strip markdown control sequences from a string, returning the plain text
/// representation.
///
/// This function only supports trivial Markdown control sequences, such as:
/// - `**bold**`
/// - `*italic*`
/// - `~strikethrough~`
/// - `\`code\``
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
