use crate::analysis::{normalize_text, normalize_with_mapping};
use std::collections::BTreeSet;

const SNIPPET_CHARS: usize = 180;
const LEAD_CHARS: usize = 45;

/// What to look for: the whole query, its words, and the terms the document matched.
fn needles(query: &str, matched_terms: &[String]) -> Vec<String> {
    let whole = normalize_text(query);
    let mut needles: Vec<String> = std::iter::once(whole.clone())
        .chain(whole.split(' ').map(str::to_string))
        .chain(matched_terms.iter().map(|term| normalize_text(term)))
        .filter(|needle| needle.chars().count() >= 2)
        .collect();
    needles.sort_by_key(|needle| std::cmp::Reverse(needle.chars().count()));
    needles.dedup();
    needles
}

/// Every occurrence of a needle in `text`, as `(needle, start, end)` in scalar values of `text`.
fn occurrences(text: &str, needles: &[String]) -> Vec<(usize, u32, u32)> {
    let normalized = normalize_with_mapping(text);
    let positions = &normalized.original_char_positions;
    let mut found = Vec::new();
    for (index, needle) in needles.iter().enumerate() {
        let length = needle.chars().count();
        for (byte, _) in normalized.text.match_indices(needle.as_str()) {
            let start = normalized.text[..byte].chars().count();
            let (Some(first), Some(last)) = (positions.get(start), positions.get(start + length - 1))
            else {
                continue;
            };
            found.push((index, *first as u32, *last as u32 + 1));
        }
    }
    found
}

/// The matched parts of `text`, ascending and never overlapping, in scalar values.
pub(super) fn highlight_ranges(text: &str, query: &str, matched_terms: &[String]) -> Vec<[u32; 2]> {
    let mut spans: Vec<[u32; 2]> = occurrences(text, &needles(query, matched_terms))
        .into_iter()
        .map(|(_, start, end)| [start, end])
        .collect();
    spans.sort_unstable();
    let mut merged: Vec<[u32; 2]> = Vec::with_capacity(spans.len());
    for span in spans {
        match merged.last_mut() {
            Some(last) if span[0] <= last[1] => last[1] = last[1].max(span[1]),
            _ => merged.push(span),
        }
    }
    merged
}

/// The window of about 180 characters holding the most different matched terms.
pub(super) fn build_snippet(content: &str, title: &str, query: &str, matched_terms: &[String]) -> String {
    let source = if content.trim().is_empty() { title } else { content };
    let characters: Vec<char> = source.chars().collect();
    if characters.len() <= SNIPPET_CHARS {
        return tidy(source);
    }
    let found = occurrences(source, &needles(query, matched_terms));
    let reach = (SNIPPET_CHARS - LEAD_CHARS) as u32;
    let best = found
        .iter()
        .map(|(_, start, _)| {
            let covered: BTreeSet<usize> = found
                .iter()
                .filter(|(_, other, _)| (*start..start + reach).contains(other))
                .map(|(needle, _, _)| *needle)
                .collect();
            (covered.len(), std::cmp::Reverse(*start))
        })
        .max();
    let anchor = best.map_or(0, |(_, std::cmp::Reverse(start))| start as usize);
    let start = anchor.saturating_sub(LEAD_CHARS);
    let end = (start + SNIPPET_CHARS).min(characters.len());
    let mut snippet = tidy(&characters[start..end].iter().collect::<String>());
    if start > 0 {
        snippet.insert(0, '…');
    }
    if end < characters.len() {
        snippet.push('…');
    }
    snippet
}

fn tidy(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{build_snippet, highlight_ranges};

    #[test]
    fn maps_nfkc_and_collapsed_whitespace_back_to_original_text() {
        let prefix = "前".repeat(100);
        let source = format!("{prefix}  ＡＢＣ 奖学金  后文{}", "后".repeat(120));
        let snippet = build_snippet(&source, "", "abc 奖学金", &[]);
        assert!(snippet.contains("ＡＢＣ 奖学金"));
        assert!(snippet.starts_with('…'));
    }

    #[test]
    fn prefers_the_window_with_the_most_different_terms() {
        let source = format!("奖学金{}奖学金 评定 办法{}", "空".repeat(300), "尾".repeat(300));
        let terms = ["奖学金".to_string(), "评定".to_string(), "办法".to_string()];
        let snippet = build_snippet(&source, "", "奖学金 评定 办法", &terms);
        assert!(snippet.contains("奖学金 评定 办法"));
    }

    #[test]
    fn highlights_merge_overlaps_and_count_scalar_values() {
        let terms = ["操作".to_string(), "作系".to_string(), "系统".to_string()];
        assert_eq!(highlight_ranges("😀操作系统基础", "操作系统", &terms), [[1, 5]]);
        assert_eq!(highlight_ranges("Ａ操作 和 系统", "操作 系统", &[]), [[1, 3], [6, 8]]);
        assert_eq!(highlight_ranges("没有命中", "操作系统", &[]), Vec::<[u32; 2]>::new());
    }

    #[test]
    fn a_term_written_differently_still_highlights_through_matched_terms() {
        let terms = ["四、六级".to_string()];
        assert_eq!(highlight_ranges("全国大学英语四、六级考试", "四六级", &terms), [[6, 10]]);
    }
}
