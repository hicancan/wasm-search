use crate::analysis::{normalize_text, normalize_with_mapping};

pub(super) fn build_snippet(
    content: &str,
    title: &str,
    query: &str,
    matched_terms: &[String],
) -> String {
    let source = if content.trim().is_empty() {
        title
    } else {
        content
    };
    let source_chars: Vec<char> = source.chars().collect();
    if source_chars.len() <= 180 {
        return source.trim().to_string();
    }

    let normalized = normalize_with_mapping(source);
    let mut needles = vec![normalize_text(query)];
    needles.extend(matched_terms.iter().map(|term| normalize_text(term)));
    let original_position = needles
        .iter()
        .filter(|term| term.chars().count() >= 2)
        .find_map(|term| normalized.text.find(term))
        .and_then(|byte| {
            let normalized_char = normalized.text[..byte].chars().count();
            normalized
                .original_char_positions
                .get(normalized_char)
                .copied()
        })
        .unwrap_or(0);
    let start = original_position.saturating_sub(45);
    let end = (start + 180).min(source_chars.len());
    let mut snippet: String = source_chars[start..end].iter().collect();
    snippet = snippet.trim().to_string();
    if start > 0 {
        snippet.insert(0, '…');
    }
    if end < source_chars.len() {
        snippet.push('…');
    }
    snippet
}

#[cfg(test)]
mod tests {
    use super::build_snippet;

    #[test]
    fn maps_nfkc_and_collapsed_whitespace_back_to_original_text() {
        let prefix = "前".repeat(100);
        let source = format!("{prefix}  ＡＢＣ　奖学金  后文{}", "后".repeat(120));
        let snippet = build_snippet(&source, "", "abc 奖学金", &[]);
        assert!(snippet.contains("ＡＢＣ　奖学金"));
        assert!(snippet.starts_with('…'));
    }
}
