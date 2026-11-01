mod codec;
mod compile;
mod compression;
mod model;

pub use compile::compile_search_bundle;
pub use model::{
    ArtifactRef, CompiledArtifact, CompiledBundle, DocumentManifest, LexiconManifest,
    SearchBundleManifest, SEARCH_BUNDLE_FORMAT,
};

pub(crate) use codec::{
    decode_content, decode_document_block, decode_document_index, decode_lexicon_block,
    decode_lexicon_index, decode_postings,
};
pub(crate) use compression::decompress_artifact;
