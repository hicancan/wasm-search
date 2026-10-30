use super::model::{
    FilterOption, FilterOptions, Query, SearchAttachment, SearchResponse, SearchResult, SortMode,
};
use super::ranking::posting_score;
use super::snippet::build_snippet;
use crate::analysis::{analyze_search_query, normalize_text, QueryAnalysis};
use crate::bundle::{
    decode_content, decode_documents, decode_lexicon, decode_postings, decompress_artifact,
};
use crate::document::{DocumentKind, DocumentMeta, Posting};
use crate::profile::Profile;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

const PARTIAL_FALLBACK_FLOOR: usize = 20;
const MAX_QUERY_LIMIT: usize = 1_000;

pub struct SearchEngine {
    profile: Profile,
    documents: Vec<DocumentMeta>,
    normalized_titles: Vec<String>,
    lexicon: HashMap<String, u32>,
    postings: HashMap<String, Vec<Posting>>,
    loaded_posting_chunks: BTreeSet<u32>,
    content: HashMap<u32, String>,
    loaded_content_chunks: BTreeSet<u32>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MatchTier {
    Partial,
    MinimumShouldMatch,
    PinyinBody,
    AllBodyTerms,
    PinyinTitle,
    AllTitleTerms,
    TitleContains,
    TitleStartsWith,
    TitleEquals,
}

impl MatchTier {
    fn relevance_band(self) -> u8 {
        match self {
            Self::TitleEquals | Self::TitleStartsWith | Self::TitleContains => 5,
            Self::AllTitleTerms => 4,
            Self::AllBodyTerms | Self::PinyinTitle => 3,
            Self::MinimumShouldMatch | Self::PinyinBody => 2,
            Self::Partial => 1,
        }
    }
}

#[derive(Default)]
struct CandidateState {
    score: f32,
    title_terms: BTreeSet<String>,
    body_terms: BTreeSet<String>,
    matched_terms: BTreeSet<String>,
}

struct RankedCandidate {
    document: u32,
    score: f32,
    tier: MatchTier,
    source_fit: bool,
    matched_terms: Vec<String>,
}

fn display_terms(terms: BTreeSet<String>) -> Vec<String> {
    let mut terms: Vec<_> = terms.into_iter().collect();
    terms.sort_by(|left, right| {
        right
            .chars()
            .count()
            .cmp(&left.chars().count())
            .then_with(|| left.cmp(right))
    });
    let mut selected: Vec<String> = Vec::new();
    for term in terms {
        if term.starts_with(crate::pinyin::PREFIX) {
            continue;
        }
        if selected.iter().any(|longer| longer.contains(&term)) {
            continue;
        }
        selected.push(term);
        if selected.len() == 12 {
            break;
        }
    }
    selected.sort();
    selected
}

fn presentation_title_key(value: &str) -> String {
    let normalized = normalize_text(value);
    let trimmed = normalized
        .trim_start_matches(|character: char| character.is_whitespace() || character == '\u{200b}');
    let without_department = trimmed
        .strip_prefix('【')
        .and_then(|rest| rest.find('】').map(|end| &rest[end + '】'.len_utf8()..]))
        .unwrap_or(trimmed);
    without_department.trim().to_string()
}

/// The exact, stable ordering for one query and its filters. Candidate
/// admission is decided before filters are applied, so narrowing a query can
/// only remove canonical results. Callers may page and hydrate the plan, but
/// cannot observe or reinterpret ranking scores.
pub struct QueryPlan {
    query: String,
    total_candidates: usize,
    ranked: Vec<RankedCandidate>,
}

pub struct QueryPreparation {
    request: Query,
    analysis: QueryAnalysis,
}

impl SearchEngine {
    pub fn new(
        documents: &[u8],
        document_bytes: u64,
        lexicon: &[u8],
        lexicon_bytes: u64,
        profile: Profile,
    ) -> Result<Self, String> {
        let documents = decode_documents(&decompress_artifact(documents, document_bytes)?)?;
        let normalized_titles = documents
            .iter()
            .map(|document| normalize_text(&document.title))
            .collect();
        Ok(Self {
            profile,
            documents,
            normalized_titles,
            lexicon: decode_lexicon(&decompress_artifact(lexicon, lexicon_bytes)?)?
                .into_iter()
                .collect(),
            postings: HashMap::new(),
            loaded_posting_chunks: BTreeSet::new(),
            content: HashMap::new(),
            loaded_content_chunks: BTreeSet::new(),
        })
    }

    pub fn document_count(&self) -> usize {
        self.documents.len()
    }

    pub fn load_postings_chunk(
        &mut self,
        chunk: u32,
        bytes: &[u8],
        decoded_bytes: u64,
    ) -> Result<(), String> {
        for (term, postings) in decode_postings(&decompress_artifact(bytes, decoded_bytes)?)? {
            if self.postings.insert(term.clone(), postings).is_some() {
                return Err(format!("duplicate term across postings chunks: {term}"));
            }
        }
        self.loaded_posting_chunks.insert(chunk);
        Ok(())
    }

    pub fn load_content_chunk(
        &mut self,
        chunk: u32,
        bytes: &[u8],
        decoded_bytes: u64,
    ) -> Result<(), String> {
        for (document, content) in decode_content(&decompress_artifact(bytes, decoded_bytes)?)? {
            if self.content.insert(document, content).is_some() {
                return Err(format!(
                    "duplicate document across content chunks: {document}"
                ));
            }
        }
        self.loaded_content_chunks.insert(chunk);
        Ok(())
    }

    pub fn clear_content(&mut self) {
        self.content.clear();
        self.loaded_content_chunks.clear();
    }

    pub fn begin_query(&self, request: &Query) -> Result<QueryPreparation, String> {
        Self::validate_request(request)?;
        Ok(QueryPreparation {
            request: request.clone(),
            analysis: analyze_search_query(&request.query, &self.profile),
        })
    }

    pub fn required_posting_chunks(&self, preparation: &QueryPreparation) -> Vec<u32> {
        preparation
            .analysis
            .retrieval_terms
            .iter()
            .filter_map(|term| self.lexicon.get(term).copied())
            .filter(|chunk| !self.loaded_posting_chunks.contains(chunk))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn required_content_chunks(
        &self,
        plan: &QueryPlan,
        offset: usize,
        limit: usize,
    ) -> Vec<u32> {
        plan.ranked
            .iter()
            .skip(offset)
            .take(limit)
            .filter_map(|candidate| self.documents.get(candidate.document as usize))
            .map(|document| document.content_chunk)
            .filter(|chunk| !self.loaded_content_chunks.contains(chunk))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn filter_options(&self) -> FilterOptions {
        let mut sources: BTreeMap<(String, String), usize> = BTreeMap::new();
        let mut categories: BTreeMap<String, usize> = BTreeMap::new();
        let mut categories_by_source: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        for document in &self.documents {
            *sources
                .entry((document.source.clone(), document.source_name.clone()))
                .or_default() += 1;
            if let Some(category) = &document.category {
                *categories.entry(category.clone()).or_default() += 1;
                *categories_by_source
                    .entry(document.source.clone())
                    .or_default()
                    .entry(category.clone())
                    .or_default() += 1;
            }
        }
        let options = |counts: BTreeMap<String, usize>| -> Vec<FilterOption> {
            counts
                .into_iter()
                .map(|(id, count)| FilterOption {
                    label: id.clone(),
                    id,
                    count,
                })
                .collect()
        };
        FilterOptions {
            sources: sources
                .into_iter()
                .map(|((id, label), count)| FilterOption { id, label, count })
                .collect(),
            categories: options(categories),
            categories_by_source: categories_by_source
                .into_iter()
                .map(|(source, counts)| (source, options(counts)))
                .collect(),
        }
    }

    fn validate_request(request: &Query) -> Result<(), String> {
        if normalize_text(&request.query).chars().count() < 2 {
            return Err("query must contain at least two normalized characters".to_string());
        }
        if !(1..=MAX_QUERY_LIMIT).contains(&request.limit) {
            return Err(format!(
                "query limit must be between 1 and {MAX_QUERY_LIMIT}"
            ));
        }
        if request
            .filters
            .source_id
            .as_deref()
            .is_some_and(|source| source.trim().is_empty() || source == "all")
        {
            return Err("sourceId must be a real source identity".to_string());
        }
        let excluded_sources = &request.filters.excluded_source_ids;
        if excluded_sources
            .iter()
            .any(|source| source.trim().is_empty() || source == "all")
        {
            return Err("excludedSourceIds must contain real source identities".to_string());
        }
        if excluded_sources.iter().collect::<HashSet<_>>().len() != excluded_sources.len() {
            return Err("excludedSourceIds must not contain duplicates".to_string());
        }
        if request
            .filters
            .source_id
            .as_ref()
            .is_some_and(|source| excluded_sources.contains(source))
        {
            return Err("sourceId must not also be excluded".to_string());
        }
        for value in [
            request.filters.published_from.as_deref(),
            request.filters.published_to.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if !valid_iso_date(value) {
                return Err(format!("invalid date filter: {value}"));
            }
        }
        if matches!(
            (
                request.filters.published_from.as_deref(),
                request.filters.published_to.as_deref()
            ),
            (Some(from), Some(to)) if from > to
        ) {
            return Err("publishedFrom must not be after publishedTo".to_string());
        }
        Ok(())
    }

    fn matches_filters(document: &DocumentMeta, request: &Query) -> bool {
        let filters = &request.filters;
        if filters
            .source_id
            .as_deref()
            .is_some_and(|source| document.source != source)
        {
            return false;
        }
        if filters.excluded_source_ids.contains(&document.source) {
            return false;
        }
        if filters
            .category
            .as_ref()
            .is_some_and(|category| document.category.as_ref() != Some(category))
        {
            return false;
        }
        let has_date_bounds = filters.published_from.is_some() || filters.published_to.is_some();
        let Some(date) = document.published_at.as_deref() else {
            return !has_date_bounds || filters.include_undated;
        };
        if filters
            .published_from
            .as_deref()
            .is_some_and(|from| date < from)
            || filters.published_to.as_deref().is_some_and(|to| date > to)
        {
            return false;
        }
        true
    }

    fn match_tier(
        &self,
        document_index: usize,
        state: &CandidateState,
        analysis: &QueryAnalysis,
    ) -> MatchTier {
        let title = &self.normalized_titles[document_index];
        let mut tier = analysis
            .phrase_groups
            .iter()
            .map(|group| {
                if title == &group.phrase {
                    MatchTier::TitleEquals
                } else if title.starts_with(&group.phrase) {
                    MatchTier::TitleStartsWith
                } else if title.contains(&group.phrase) {
                    MatchTier::TitleContains
                } else if !group.title_terms.is_empty()
                    && group
                        .title_terms
                        .iter()
                        .all(|term| state.title_terms.contains(term))
                {
                    MatchTier::AllTitleTerms
                } else if !group.body_terms.is_empty()
                    && group
                        .body_terms
                        .iter()
                        .all(|term| state.body_terms.contains(term))
                {
                    MatchTier::AllBodyTerms
                } else {
                    let matched = group
                        .body_terms
                        .iter()
                        .filter(|term| {
                            state.title_terms.contains(*term) || state.body_terms.contains(*term)
                        })
                        .count();
                    let required = (group.body_terms.len() * 3).div_ceil(5);
                    if !group.body_terms.is_empty() && matched >= required {
                        MatchTier::MinimumShouldMatch
                    } else {
                        MatchTier::Partial
                    }
                }
            })
            .max()
            .unwrap_or(MatchTier::Partial);

        // Transliteration is a fallback below literal Chinese/Latin title
        // matches, independent of publication date and repetition counts.
        if state
            .title_terms
            .iter()
            .any(|term| term.starts_with(crate::pinyin::PREFIX))
        {
            tier = tier.max(MatchTier::PinyinTitle);
        } else if state
            .body_terms
            .iter()
            .any(|term| term.starts_with(crate::pinyin::PREFIX))
        {
            tier = tier.max(MatchTier::PinyinBody);
        }

        if self.documents[document_index].kind == DocumentKind::Link
            && tier > MatchTier::MinimumShouldMatch
        {
            tier = MatchTier::MinimumShouldMatch;
        }
        if self.documents[document_index].published_at.is_none() && tier > MatchTier::AllTitleTerms
        {
            tier = MatchTier::AllTitleTerms;
        }
        tier
    }

    pub fn plan_query(&self, preparation: &QueryPreparation) -> Result<QueryPlan, String> {
        let request = &preparation.request;
        let analysis = &preparation.analysis;
        let missing = analysis
            .retrieval_terms
            .iter()
            .filter_map(|term| self.lexicon.get(term).copied())
            .filter(|chunk| !self.loaded_posting_chunks.contains(chunk))
            .collect::<BTreeSet<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "required postings chunks are not loaded: {missing:?}"
            ));
        }

        let mut states: Vec<Option<CandidateState>> = std::iter::repeat_with(|| None)
            .take(self.documents.len())
            .collect();
        let mut touched = Vec::new();
        for term in &analysis.retrieval_terms {
            let Some(postings) = self.postings.get(term) else {
                continue;
            };
            for posting in postings {
                if self.documents.get(posting.document as usize).is_none() {
                    return Err(format!(
                        "posting references missing document: {}",
                        posting.document
                    ));
                }
                let slot = &mut states[posting.document as usize];
                if slot.is_none() {
                    *slot = Some(CandidateState::default());
                    touched.push(posting.document);
                }
                let state = slot.as_mut().expect("candidate state");
                state.score += posting_score(
                    term,
                    *posting,
                    postings.len(),
                    self.documents.len(),
                    term == &analysis.normalized,
                );
                if posting.title_hits > 0 {
                    state.title_terms.insert(term.clone());
                }
                if posting.body_hits > 0 {
                    state.body_terms.insert(term.clone());
                }
                state.matched_terms.insert(term.clone());
            }
        }

        let mut ranked: Vec<_> = touched
            .into_iter()
            .map(|document| {
                let state = states[document as usize]
                    .take()
                    .expect("touched candidate state");
                RankedCandidate {
                    document,
                    score: state.score,
                    tier: self.match_tier(document as usize, &state, analysis),
                    source_fit: analysis
                        .preferred_sources
                        .contains(&self.documents[document as usize].source),
                    matched_terms: display_terms(state.matched_terms),
                }
            })
            .collect();

        // Candidate admission is a property of the analyzed query, never of
        // the selected source, category, date range, or sort mode. Complete and
        // minimum-coverage matches are always admitted. Partial matches are a
        // fallback only when the unfiltered query has too few stronger
        // candidates; filters cannot turn that fallback on or off.
        let minimum_matches = ranked
            .iter()
            .filter(|candidate| candidate.tier >= MatchTier::MinimumShouldMatch)
            .count();
        if minimum_matches >= PARTIAL_FALLBACK_FLOOR {
            ranked.retain(|candidate| candidate.tier >= MatchTier::MinimumShouldMatch);
        }
        ranked.retain(|candidate| {
            Self::matches_filters(&self.documents[candidate.document as usize], request)
        });

        match request.sort {
            SortMode::Relevance => ranked.sort_by(|left, right| {
                right
                    .tier
                    .relevance_band()
                    .cmp(&left.tier.relevance_band())
                    .then_with(|| right.source_fit.cmp(&left.source_fit))
                    .then_with(|| {
                        self.documents[right.document as usize]
                            .published_at
                            .cmp(&self.documents[left.document as usize].published_at)
                    })
                    .then_with(|| right.tier.cmp(&left.tier))
                    .then_with(|| right.score.total_cmp(&left.score))
                    .then_with(|| {
                        self.documents[left.document as usize]
                            .id
                            .cmp(&self.documents[right.document as usize].id)
                    })
            }),
            SortMode::DateDesc => ranked.sort_by(|left, right| {
                self.documents[right.document as usize]
                    .published_at
                    .cmp(&self.documents[left.document as usize].published_at)
                    .then_with(|| right.tier.cmp(&left.tier))
                    .then_with(|| right.score.total_cmp(&left.score))
                    .then_with(|| {
                        self.documents[left.document as usize]
                            .id
                            .cmp(&self.documents[right.document as usize].id)
                    })
            }),
        }

        let mut seen_urls = HashSet::new();
        let mut seen_presentations = HashSet::new();
        ranked.retain(|candidate| {
            let document = &self.documents[candidate.document as usize];
            if !seen_urls.insert(document.url.clone()) {
                return false;
            }
            let Some(date) = &document.published_at else {
                return true;
            };
            let title = presentation_title_key(&document.title);
            if title.chars().count() < 6 {
                return true;
            }
            seen_presentations.insert((date.clone(), title))
        });
        Ok(QueryPlan {
            query: request.query.clone(),
            total_candidates: ranked.len(),
            ranked,
        })
    }

    fn result(
        &self,
        plan: &QueryPlan,
        candidate: &RankedCandidate,
        with_snippet: bool,
    ) -> Result<SearchResult, String> {
        let document = &self.documents[candidate.document as usize];
        let snippet = if with_snippet {
            if !self.loaded_content_chunks.contains(&document.content_chunk) {
                return Err(format!(
                    "required content chunk is not loaded: {}",
                    document.content_chunk
                ));
            }
            let content = self
                .content
                .get(&candidate.document)
                .map(String::as_str)
                .unwrap_or("");
            Some(build_snippet(
                content,
                &document.title,
                &plan.query,
                &candidate.matched_terms,
            ))
        } else {
            None
        };
        Ok(SearchResult {
            id: document.id.clone(),
            source: document.source.clone(),
            source_name: document.source_name.clone(),
            url: document.url.clone(),
            title: document.title.clone(),
            published_at: document.published_at.clone(),
            updated_at: document.updated_at.clone(),
            section: document.section.clone(),
            kind: document.kind,
            category: document.category.clone(),
            snippet,
            matched_terms: if with_snippet {
                candidate.matched_terms.clone()
            } else {
                Vec::new()
            },
            attachments: if with_snippet {
                document
                    .attachments
                    .iter()
                    .map(|attachment| SearchAttachment {
                        url: attachment.url.clone(),
                        name: attachment.name.clone(),
                        extension: attachment.extension.clone(),
                    })
                    .collect()
            } else {
                Vec::new()
            },
        })
    }

    fn page(
        &self,
        plan: &QueryPlan,
        offset: usize,
        limit: usize,
        with_snippet: bool,
    ) -> Result<SearchResponse, String> {
        let results = plan
            .ranked
            .iter()
            .skip(offset)
            .take(limit)
            .map(|candidate| self.result(plan, candidate, with_snippet))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SearchResponse {
            total_candidates: plan.total_candidates,
            results,
        })
    }

    pub fn result_shells(
        &self,
        plan: &QueryPlan,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResponse, String> {
        self.page(plan, offset, limit, false)
    }

    pub fn hydrate_results(
        &self,
        plan: &QueryPlan,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResponse, String> {
        self.page(plan, offset, limit, true)
    }

    pub fn search(&self, request: &Query) -> Result<SearchResponse, String> {
        let preparation = self.begin_query(request)?;
        let plan = self.plan_query(&preparation)?;
        self.hydrate_results(&plan, 0, request.limit)
    }
}

fn valid_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let parse = |range: std::ops::Range<usize>| {
        std::str::from_utf8(&bytes[range]).ok()?.parse::<u32>().ok()
    };
    let Some(year) = parse(0..4) else {
        return false;
    };
    let Some(month) = parse(5..7) else {
        return false;
    };
    let Some(day) = parse(8..10) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=max_day).contains(&day)
}

#[cfg(test)]
mod tests {
    use super::valid_iso_date;

    #[test]
    fn validates_real_calendar_dates() {
        assert!(valid_iso_date("2026-07-30"));
        assert!(valid_iso_date("2024-02-29"));
        assert!(!valid_iso_date("2026-02-29"));
        assert!(!valid_iso_date("2026-13-01"));
    }
}
