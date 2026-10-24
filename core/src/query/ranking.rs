use crate::analysis::token_weight;
use crate::document::Posting;

pub(super) fn posting_score(
    token: &str,
    posting: Posting,
    document_frequency: usize,
    total: usize,
    exact_query_term: bool,
) -> f32 {
    let idf = (((total + 1) as f32 / (document_frequency + 1) as f32).ln() + 1.0).max(0.1);
    let title_weight = if exact_query_term { 64.0 } else { 6.0 };
    let hits = posting.title_hits as f32 * title_weight + posting.body_hits.min(12) as f32;
    token_weight(token) * idf * hits
}

#[cfg(test)]
mod tests {
    use super::posting_score;
    use crate::document::Posting;

    #[test]
    fn exact_query_phrase_in_title_outranks_repeated_body_mentions() {
        let title = posting_score(
            "校园网",
            Posting {
                document: 0,
                title_hits: 1,
                body_hits: 0,
            },
            100,
            10_000,
            true,
        );
        let body = posting_score(
            "校园网",
            Posting {
                document: 1,
                title_hits: 0,
                body_hits: 12,
            },
            100,
            10_000,
            true,
        );
        assert!(title > body);
    }
}
