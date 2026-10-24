use serde::{Deserialize, Serialize};

/// Corpus-specific query knowledge, supplied by whoever builds the bundle and
/// shipped inside it, so native and WASM queries apply the same rules.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default)]
    pub aliases: Vec<Alias>,
}

/// A query equal to one of `terms` searches for `phrases` instead, and ranks
/// documents from `prefer_sources` first within a relevance tier.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Alias {
    pub terms: Vec<String>,
    pub phrases: Vec<String>,
    #[serde(default)]
    pub prefer_sources: Vec<String>,
}
