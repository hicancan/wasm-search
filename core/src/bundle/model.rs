use crate::profile::Profile;
use serde::{Deserialize, Serialize};

pub const SEARCH_BUNDLE_FORMAT: &str = "wasm-search-bundle-v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub path: String,
    pub bytes: u64,
    pub decoded_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchBundleManifest {
    pub format: String,
    pub documents: ArtifactRef,
    pub lexicon: ArtifactRef,
    pub postings: Vec<ArtifactRef>,
    pub content: Vec<ArtifactRef>,
    pub profile: Profile,
}

#[derive(Clone, Debug)]
pub struct CompiledArtifact {
    pub reference: ArtifactRef,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct CompiledBundle {
    pub manifest: SearchBundleManifest,
    pub artifacts: Vec<CompiledArtifact>,
    pub document_count: u32,
    pub lexicon_terms: usize,
}

impl CompiledBundle {
    pub fn total_artifact_bytes(&self) -> u64 {
        self.artifacts
            .iter()
            .map(|artifact| artifact.reference.bytes)
            .sum()
    }
}
