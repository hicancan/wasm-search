use crate::document::DocumentKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SearchFilters {
    /// When not empty, only these sources.
    #[serde(default)]
    pub source_ids: Vec<String>,
    #[serde(default)]
    pub excluded_source_ids: Vec<String>,
    pub category: Option<String>,
    pub published_from: Option<String>,
    pub published_to: Option<String>,
    #[serde(default)]
    pub include_undated: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SortMode {
    #[default]
    Relevance,
    DateDesc,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Query {
    pub query: String,
    /// How many ranked results to skip, for paging.
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub sort: SortMode,
    #[serde(default)]
    pub filters: SearchFilters,
}

fn default_limit() -> usize {
    30
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchAttachment {
    pub url: String,
    pub name: String,
    pub extension: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub source: String,
    pub source_name: String,
    pub url: String,
    pub title: String,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    pub section: Option<String>,
    pub kind: DocumentKind,
    pub category: Option<String>,
    pub snippet: Option<String>,
    /// Matched `[start, end)` ranges of the title, in Unicode scalar values.
    pub title_highlights: Vec<[u32; 2]>,
    /// The same for the snippet.
    pub snippet_highlights: Vec<[u32; 2]>,
    pub attachments: Vec<SearchAttachment>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub total_candidates: usize,
    pub results: Vec<SearchResult>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FilterOption {
    pub id: String,
    pub label: String,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterOptions {
    pub sources: Vec<FilterOption>,
    pub categories: Vec<FilterOption>,
    /// Category counts scoped to each source, so a source-local view never
    /// shows whole-corpus counts.
    pub categories_by_source: std::collections::BTreeMap<String, Vec<FilterOption>>,
}
