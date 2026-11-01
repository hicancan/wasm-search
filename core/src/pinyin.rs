//! Transliteration is compiled into the ordinary lexicon/postings. Readers do
//! not scan titles or carry a corpus-specific pronunciation dictionary.
use crate::analysis::normalize_text;
use pinyin::ToPinyin;
use std::collections::BTreeSet;

pub(crate) const PREFIX: &str = "\u{1f}py:";

/// Plain syllables may be typed together or separated by whitespace. Keep this
/// separate from normal text analysis so Latin phrases retain their semantics.
pub(crate) fn query_term(value: &str) -> Option<String> {
    let compact: String = normalize_text(value)
        .chars()
        .filter(|character| !character.is_whitespace())
        .map(|character| if character == 'ü' { 'v' } else { character })
        .collect();
    (compact.len() >= 2 && compact.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| format!("{PREFIX}{compact}"))
}

pub(crate) fn document_terms(value: &str, max_n: usize) -> Vec<String> {
    fn flush(run: &mut Vec<(String, String, bool)>, max_n: usize, terms: &mut BTreeSet<String>) {
        for start in 0..run.len() {
            // Match the same character windows as Chinese analysis. Also index
            // a complete short name, without a quadratic index for long prose.
            let mut ends: Vec<usize> = (start + 1..=run.len().min(start + max_n)).collect();
            if start == 0 && run.len() <= 32 {
                ends.push(run.len());
            }
            for end in ends {
                let window = &run[start..end];
                if !window.iter().any(|(_, _, han)| *han) {
                    continue;
                }
                for initials in [false, true] {
                    let text: String = window
                        .iter()
                        .map(|(full, initial, _)| {
                            if initials {
                                initial.as_str()
                            } else {
                                full.as_str()
                            }
                        })
                        .collect();
                    if text.len() >= 2 {
                        terms.insert(format!("{PREFIX}{text}"));
                    }
                }
            }
        }
        run.clear();
    }

    let mut terms = BTreeSet::new();
    let mut run = Vec::new();
    for character in normalize_text(value).chars() {
        if let Some(pronunciation) = character.to_pinyin() {
            let plain = pronunciation.plain().replace('ü', "v");
            run.push((plain.clone(), plain[..1].to_string(), true));
        } else if character.is_ascii_alphanumeric() {
            run.push((character.to_string(), character.to_string(), false));
        } else {
            flush(&mut run, max_n, &mut terms);
        }
    }
    flush(&mut run, max_n, &mut terms);
    terms.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_names_and_mixed_digits_without_crossing_punctuation() {
        let terms = document_terms("图书馆 教3", 4);
        for expected in ["tushuguan", "tsg", "jiao3", "j3"] {
            assert!(terms.contains(&format!("{PREFIX}{expected}")), "{expected}");
        }
        assert!(!terms.contains(&format!("{PREFIX}tushuguanjiao3")));
        assert_eq!(
            query_term("TU SHU GUAN"),
            Some(format!("{PREFIX}tushuguan"))
        );
        assert_eq!(query_term("图书馆"), None);
        assert_eq!(query_term("LÜ GUAN"), Some(format!("{PREFIX}lvguan")));
        assert!(document_terms("旅馆", 4).contains(&format!("{PREFIX}lvguan")));
    }
}
