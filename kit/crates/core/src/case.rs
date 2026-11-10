//! Identifier case conversion.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Camel,
    Pascal,
    Snake,
    ScreamingSnake,
    Kebab,
    Title,
    Lower,
    Upper,
}

impl Case {
    pub const ALL: [Case; 8] = [
        Case::Camel,
        Case::Pascal,
        Case::Snake,
        Case::ScreamingSnake,
        Case::Kebab,
        Case::Title,
        Case::Lower,
        Case::Upper,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Case::Camel => "camelCase",
            Case::Pascal => "PascalCase",
            Case::Snake => "snake_case",
            Case::ScreamingSnake => "SCREAMING_SNAKE",
            Case::Kebab => "kebab-case",
            Case::Title => "Title Case",
            Case::Lower => "lower case",
            Case::Upper => "UPPER CASE",
        }
    }
}

/// Splits text into words at separators (anything not alphanumeric), at
/// lower→upper boundaries (`fooBar`) and at the end of acronyms (`HTTPServer`
/// → `HTTP`, `Server`). Digits stay attached to the preceding word.
pub fn words(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut words = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        if let Some(&prev) = current.chars().last().as_ref() {
            let next = chars.get(i + 1).copied();
            let lower_to_upper = (prev.is_lowercase() || prev.is_numeric()) && c.is_uppercase();
            let acronym_end =
                prev.is_uppercase() && c.is_uppercase() && next.is_some_and(char::is_lowercase);
            if lower_to_upper || acronym_end {
                words.push(std::mem::take(&mut current));
            }
        }
        current.push(c);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect(),
        None => String::new(),
    }
}

/// Converts each line of `text` separately, so a list of identifiers can be
/// converted at once.
pub fn convert(text: &str, case: Case) -> String {
    text.lines()
        .map(|line| convert_line(line, case))
        .collect::<Vec<_>>()
        .join("\n")
}

fn convert_line(line: &str, case: Case) -> String {
    let words = words(line);
    let lower = || words.iter().map(|w| w.to_lowercase());
    match case {
        Case::Camel => words
            .iter()
            .enumerate()
            .map(|(i, w)| {
                if i == 0 {
                    w.to_lowercase()
                } else {
                    capitalize(w)
                }
            })
            .collect(),
        Case::Pascal => words.iter().map(|w| capitalize(w)).collect(),
        Case::Snake => lower().collect::<Vec<_>>().join("_"),
        Case::ScreamingSnake => words
            .iter()
            .map(|w| w.to_uppercase())
            .collect::<Vec<_>>()
            .join("_"),
        Case::Kebab => lower().collect::<Vec<_>>().join("-"),
        Case::Title => words
            .iter()
            .map(|w| capitalize(w))
            .collect::<Vec<_>>()
            .join(" "),
        Case::Lower => lower().collect::<Vec<_>>().join(" "),
        Case::Upper => words
            .iter()
            .map(|w| w.to_uppercase())
            .collect::<Vec<_>>()
            .join(" "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_words() {
        assert_eq!(words("fooBarBaz"), ["foo", "Bar", "Baz"]);
        assert_eq!(words("HTTPServerError"), ["HTTP", "Server", "Error"]);
        assert_eq!(
            words("snake_case-and kebab"),
            ["snake", "case", "and", "kebab"]
        );
        assert_eq!(words("version2Update"), ["version2", "Update"]);
        assert_eq!(words("  __  "), Vec::<String>::new());
        assert_eq!(words("ÉtéChaud"), ["Été", "Chaud"]);
    }

    #[test]
    fn converts_to_every_case() {
        let input = "user_account ID";
        assert_eq!(convert(input, Case::Camel), "userAccountId");
        assert_eq!(convert(input, Case::Pascal), "UserAccountId");
        assert_eq!(convert(input, Case::Snake), "user_account_id");
        assert_eq!(convert(input, Case::ScreamingSnake), "USER_ACCOUNT_ID");
        assert_eq!(convert(input, Case::Kebab), "user-account-id");
        assert_eq!(convert(input, Case::Title), "User Account Id");
        assert_eq!(convert(input, Case::Lower), "user account id");
        assert_eq!(convert(input, Case::Upper), "USER ACCOUNT ID");
    }

    #[test]
    fn converts_each_line() {
        assert_eq!(convert("fooBar\nbaz-qux", Case::Snake), "foo_bar\nbaz_qux");
    }

    #[test]
    fn empty_input() {
        assert_eq!(convert("", Case::Camel), "");
        assert_eq!(convert("---", Case::Kebab), "");
    }

    #[test]
    fn labels_are_distinct() {
        let labels: std::collections::HashSet<_> = Case::ALL.iter().map(|c| c.label()).collect();
        assert_eq!(labels.len(), Case::ALL.len());
    }
}
