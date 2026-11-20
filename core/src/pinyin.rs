//! Transliteration is compiled into the ordinary lexicon/postings. Readers do
//! not scan titles or carry a corpus-specific pronunciation dictionary.
use crate::analysis::normalize_text;
use pinyin::ToPinyin;
use std::collections::BTreeSet;

pub(crate) const PREFIX: &str = "\u{1f}py:";
/// A transliteration of a whole title. Window terms say a title contains the
/// query; this says the title *is* the query, as a literal equal title does,
/// so `tsg` finds the place 图书馆 above every notice that mentions it.
pub(crate) const WHOLE: &str = "\u{1f}py:=";

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

/// The whole-title term a query would match, beside its window term.
pub(crate) fn whole_query_term(value: &str) -> Option<String> {
    query_term(value).map(|term| format!("{WHOLE}{}", &term[PREFIX.len()..]))
}

/// Full and initial transliterations of a title that is one unbroken name of at
/// most 32 characters, holding at least one Chinese character.
pub(crate) fn whole_terms(title: &str) -> Vec<String> {
    let normalized = normalize_text(title);
    let characters: Vec<char> = normalized.chars().collect();
    if characters.is_empty() || characters.len() > 32 {
        return Vec::new();
    }
    let (mut full, mut initials, mut han) = (String::new(), String::new(), false);
    for character in characters {
        if let Some(pronunciation) = character.to_pinyin() {
            let plain = pronunciation.plain().replace('ü', "v");
            initials.push_str(&plain[..1]);
            full.push_str(&plain);
            han = true;
        } else if character.is_ascii_alphanumeric() {
            full.push(character);
            initials.push(character);
        } else {
            return Vec::new();
        }
    }
    if !han {
        return Vec::new();
    }
    let mut terms = vec![format!("{WHOLE}{full}")];
    if initials != full {
        terms.push(format!("{WHOLE}{initials}"));
    }
    terms
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

    #[test]
    fn whole_terms_name_only_an_unbroken_title() {
        assert_eq!(whole_terms("图书馆"), [format!("{WHOLE}tushuguan"), format!("{WHOLE}tsg")]);
        assert_eq!(whole_terms("教3"), [format!("{WHOLE}jiao3"), format!("{WHOLE}j3")]);
        assert!(whole_terms("图书馆 开放时间").is_empty());
        assert!(whole_terms("教1-101").is_empty());
        assert!(whole_terms("library").is_empty());
        assert_eq!(whole_query_term("T S G"), Some(format!("{WHOLE}tsg")));
    }
}
