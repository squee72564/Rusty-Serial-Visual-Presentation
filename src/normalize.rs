#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedDocument {
    pub text: String,
}

pub fn normalize_text(input: &str) -> NormalizedDocument {
    let joined = join_hyphenated_line_breaks(input);
    let paragraphs = joined
        .split(|c| c == '\n' || c == '\r')
        .map(collapse_inline_whitespace)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    let mut text = String::new();
    for paragraph in paragraphs {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&paragraph);
    }

    NormalizedDocument { text }
}

fn join_hyphenated_line_breaks(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '-' {
            let mut lookahead = chars.clone();
            let mut saw_line_break = false;
            while matches!(lookahead.peek(), Some(' ' | '\t')) {
                lookahead.next();
            }
            if matches!(lookahead.peek(), Some('\r')) {
                saw_line_break = true;
                lookahead.next();
            }
            if matches!(lookahead.peek(), Some('\n')) {
                saw_line_break = true;
                lookahead.next();
            }

            if saw_line_break {
                chars = lookahead;
                while matches!(chars.peek(), Some(' ' | '\t')) {
                    chars.next();
                }
                continue;
            }
        }

        output.push(ch);
    }

    output
}

fn collapse_inline_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_whitespace_and_joins_hyphenated_lines() {
        let normalized = normalize_text("A hyphen-\nated   word.\n\nNext\t paragraph.");

        assert_eq!(normalized.text, "A hyphenated word.\n\nNext paragraph.");
    }
}
