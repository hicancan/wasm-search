mod codec;
mod compile;
mod compression;
mod model;

pub use compile::{compile_search_bundle, compile_search_bundle_with};
pub use model::{
    ArtifactRef, BlockRef, BlockTables, CompiledBundle, SearchBundleManifest, SEARCH_BUNDLE_FORMAT,
};

pub(crate) use codec::{
    decode_index, decode_page, decode_postings, BundleIndex, RankRow,
};
pub(crate) use compression::decompress_artifact;
