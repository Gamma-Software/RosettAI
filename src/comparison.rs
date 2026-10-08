use std::borrow::Cow;

#[derive(Debug, PartialEq)]
pub(crate) enum Comparison {
    Lines {
        common: usize,
        local: usize,
        expected: usize,
    },
    TooLarge,
}

impl Comparison {
    pub(crate) fn description(&self) -> String {
        match *self {
            Self::Lines {
                common,
                local,
                expected,
            } => {
                let total = local + expected;
                let percent = if total == 0 {
                    100.0
                } else {
                    200.0 * common as f64 / total as f64
                };
                format!(
                    "{percent:.1}% similar · local diff: +{} / −{} lines",
                    local - common,
                    expected - common
                )
            }
            Self::TooLarge => "similarity unavailable: comparison too large".into(),
        }
    }
}

// Compare the projection's content, excluding rai's ownership digest. JSON is
// normalized so key order and indentation do not count as content changes.
fn content_without_marker<'a>(content: &'a str, kind: &str) -> Cow<'a, str> {
    if kind == "json"
        && let Ok(mut value) = serde_json::from_str::<serde_json::Value>(content)
    {
        if let Some(object) = value.as_object_mut() {
            object.remove("_rai_generated_sha256");
        }
        return Cow::Owned(serde_json::to_string_pretty(&value).unwrap());
    }
    let (first, rest) = content.split_once('\n').unwrap_or((content, ""));
    if first.starts_with("<!-- rai-generated sha256:")
        || first.starts_with("# rai-generated sha256:")
        || first.starts_with("// rai-generated sha256:")
    {
        Cow::Borrowed(rest)
    } else {
        Cow::Borrowed(content)
    }
}

pub(crate) fn compare(local: &str, expected: &str, kind: &str) -> Comparison {
    let local = content_without_marker(local, kind);
    let expected = content_without_marker(expected, kind);
    let a: Vec<_> = local.lines().collect();
    let b: Vec<_> = expected.lines().collect();
    let prefix = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    let a_rest = &a[prefix..];
    let b_rest = &b[prefix..];
    let suffix = a_rest
        .iter()
        .rev()
        .zip(b_rest.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let a_middle = &a_rest[..a_rest.len() - suffix];
    let b_middle = &b_rest[..b_rest.len() - suffix];
    // Bound work for unrelated, unusually large configuration files. Never show
    // an approximate percentage as though it were an exact comparison.
    if a_middle.len().saturating_mul(b_middle.len()) > 4_000_000 {
        return Comparison::TooLarge;
    }
    let (rows, columns) = if a_middle.len() >= b_middle.len() {
        (a_middle, b_middle)
    } else {
        (b_middle, a_middle)
    };
    let mut matches = vec![0; columns.len() + 1];
    for line in rows {
        let mut diagonal = 0;
        for (index, other) in columns.iter().enumerate() {
            let previous = matches[index + 1];
            matches[index + 1] = if line == other {
                diagonal + 1
            } else {
                previous.max(matches[index])
            };
            diagonal = previous;
        }
    }
    Comparison::Lines {
        common: prefix + suffix + matches[columns.len()],
        local: a.len(),
        expected: b.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn additions_deletions_and_replacements_have_a_direction() {
        assert_eq!(
            compare("a\nb\nc\n", "a\nb\n", "markdown").description(),
            "80.0% similar · local diff: +1 / −0 lines"
        );
        assert_eq!(
            compare("a\nb\n", "a\nb\nc\n", "markdown").description(),
            "80.0% similar · local diff: +0 / −1 lines"
        );
        assert_eq!(
            compare("a\nnew\nc\n", "a\nold\nc\n", "markdown").description(),
            "66.7% similar · local diff: +1 / −1 lines"
        );
    }

    #[test]
    fn repeated_and_reordered_lines_are_counted_in_sequence() {
        assert_eq!(
            compare("a\nb\na\n", "b\na\nb\n", "markdown"),
            Comparison::Lines {
                common: 2,
                local: 3,
                expected: 3
            }
        );
    }

    #[test]
    fn empty_equal_and_unrelated_contents_are_handled() {
        assert!(
            compare("", "", "markdown")
                .description()
                .starts_with("100.0%")
        );
        assert!(
            compare("a\n", "", "markdown")
                .description()
                .starts_with("0.0%")
        );
        assert!(
            compare("a\n", "b\n", "markdown")
                .description()
                .starts_with("0.0%")
        );
        assert!(
            compare("écriture\r\n", "écriture\n", "markdown")
                .description()
                .starts_with("100.0%")
        );
        assert!(
            compare("a \n", "a\n", "markdown")
                .description()
                .starts_with("0.0%")
        );
    }

    #[test]
    fn ownership_headers_and_json_metadata_do_not_reduce_similarity() {
        for header in [
            "<!-- rai-generated sha256:old -->",
            "# rai-generated sha256:old",
            "// rai-generated sha256:old",
        ] {
            assert!(
                compare(&format!("{header}\nsame\n"), "same\n", "markdown")
                    .description()
                    .starts_with("100.0%")
            );
        }
        assert!(
            compare(
                r#"{"b":2,"_rai_generated_sha256":"bad","a":1}"#,
                r#"{"a":1,"b":2,"_rai_generated_sha256":"good"}"#,
                "json"
            )
            .description()
            .starts_with("100.0%")
        );
    }

    #[test]
    fn large_differences_are_bounded_but_large_shared_contents_are_compared() {
        let a = "a\n".repeat(2500);
        let b = "b\n".repeat(2500);
        assert_eq!(compare(&a, &b, "markdown"), Comparison::TooLarge);
        assert!(
            compare(&format!("{a}extra\n"), &a, "markdown")
                .description()
                .contains("+1 / −0 lines")
        );
    }
}
