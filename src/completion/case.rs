pub fn restore_case(token: &str, word: &str, capitalize_sentence: bool) -> String {
    let letters: Vec<char> = token.chars().filter(|c| c.is_alphabetic()).collect();
    if !letters.is_empty() && letters.iter().all(|c| c.is_uppercase()) {
        return word.to_uppercase();
    }

    let first_upper = token
        .chars()
        .find(|c| c.is_alphabetic())
        .map(|c| c.is_uppercase())
        .unwrap_or(capitalize_sentence);

    if !first_upper {
        return word.to_string();
    }

    let mut chars = word.chars();
    match chars.next() {
        Some(f) => {
            let mut out: String = f.to_uppercase().collect();
            out.push_str(&chars.as_str().to_lowercase());
            out
        }
        None => word.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_caps_token() {
        assert_eq!(restore_case("HEL", "hello", false), "HELLO");
    }

    #[test]
    fn title_case_token() {
        assert_eq!(restore_case("Hel", "hello", false), "Hello");
    }

    #[test]
    fn lower_token_keeps_model_case() {
        assert_eq!(restore_case("hel", "hello", false), "hello");
    }

    #[test]
    fn empty_token_sentence_start() {
        assert_eq!(restore_case("", "hello", true), "Hello");
        assert_eq!(restore_case("", "hello", false), "hello");
    }
}
