/// Split a command argument string into whitespace-separated tokens.
#[allow(dead_code)]
pub fn parse_args(text: &str) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Split `input` into the first whitespace-delimited token and everything after.
///
/// Returns `None` when `input` contains fewer than two tokens.
///
/// # Examples
/// ```
/// assert_eq!(
///     parse_first_and_rest("PROJ-1 fix the thing"),
///     Some(("PROJ-1".into(), "fix the thing".into()))
/// );
/// ```
pub fn parse_first_and_rest(input: &str) -> Option<(String, String)> {
    let mut iter = input.trim().splitn(2, char::is_whitespace);
    let first = iter.next()?.trim().to_string();
    let rest = iter.next()?.trim().to_string();
    if first.is_empty() || rest.is_empty() {
        return None;
    }
    Some((first, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_args_empty() {
        assert_eq!(parse_args(""), Vec::<String>::new());
        assert_eq!(parse_args("   "), Vec::<String>::new());
    }

    #[test]
    fn test_parse_args_multiple() {
        assert_eq!(parse_args("a b  c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_first_and_rest() {
        assert_eq!(
            parse_first_and_rest("KEY-1 do something"),
            Some(("KEY-1".into(), "do something".into()))
        );
    }

    #[test]
    fn test_parse_first_and_rest_none() {
        assert_eq!(parse_first_and_rest("KEY-1"), None);
        assert_eq!(parse_first_and_rest(""), None);
    }
}
