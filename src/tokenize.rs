use crate::normalize::NormalizedDocument;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub paragraph_index: usize,
}

pub fn tokenize(document: &NormalizedDocument) -> Vec<Token> {
    let mut tokens = Vec::new();

    for (paragraph_index, paragraph) in document.text.split("\n\n").enumerate() {
        tokens.extend(paragraph.split_whitespace().filter_map(|text| {
            has_readable_character(text).then(|| Token {
                text: text.to_string(),
                paragraph_index,
            })
        }));
    }

    tokens
}

fn has_readable_character(text: &str) -> bool {
    text.chars().any(|ch| ch.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_punctuation_attached() {
        let document = NormalizedDocument {
            text: "Hello, don't stop. Well-known U.S. 3.14".into(),
        };

        let words = tokenize(&document)
            .into_iter()
            .map(|token| token.text)
            .collect::<Vec<_>>();

        assert_eq!(
            words,
            vec!["Hello,", "don't", "stop.", "Well-known", "U.S.", "3.14"]
        );
    }
}
