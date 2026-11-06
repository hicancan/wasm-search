use crate::profile::Profile;
use serde::{Deserialize, Serialize};

pub const SEARCH_BUNDLE_FORMAT: &str = "wasm-search-bundle-v3";

/// A byte range of a pack file, compressed on its own.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub pack: u32,
    pub offset: u64,
    pub bytes: u64,
    pub decoded_bytes: u64,
}

/// What a client must fetch and hand back under `id`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct BlockRef {
    pub id: u32,
    pub pack: u32,
    pub offset: u64,
    pub bytes: u64,
    pub decoded_bytes: u64,
}

impl BlockRef {
    pub fn new(id: u32, artifact: ArtifactRef) -> Self {
        Self {
            id,
            pack: artifact.pack,
            offset: artifact.offset,
            bytes: artifact.bytes,
            decoded_bytes: artifact.decoded_bytes,
        }
    }
}

/// The manifest names the index and the packs. Everything else is inside the index,
/// so one small read at startup is all a client needs before its first query.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchBundleManifest {
    pub format: String,
    pub index: ArtifactRef,
    pub packs: Vec<String>,
    pub profile: Profile,
}

/// Every block's place in the packs, in the order they are written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockTables {
    pub lexicon: Vec<ArtifactRef>,
    pub postings: Vec<ArtifactRef>,
    pub documents: Vec<ArtifactRef>,
    pub content: Vec<ArtifactRef>,
}

#[derive(Clone, Debug)]
pub struct CompiledBundle {
    pub manifest: SearchBundleManifest,
    pub packs: Vec<Vec<u8>>,
    pub tables: BlockTables,
    pub document_count: u32,
    pub lexicon_terms: usize,
}

impl CompiledBundle {
    pub fn total_artifact_bytes(&self) -> u64 {
        self.packs.iter().map(|pack| pack.len() as u64).sum()
    }

    pub fn read(&self, artifact: ArtifactRef) -> &[u8] {
        let start = artifact.offset as usize;
        &self.packs[artifact.pack as usize][start..start + artifact.bytes as usize]
    }
}
