use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use wasm_search_core::{
    ArtifactRef, CompiledBundle, SearchBundleManifest, SearchEngine, SEARCH_BUNDLE_FORMAT,
};

fn read(bundle: &Path, manifest: &SearchBundleManifest, artifact: ArtifactRef) -> Result<Vec<u8>> {
    let pack = fs::read(bundle.join(&manifest.packs[artifact.pack as usize]))
        .with_context(|| format!("missing pack {}", artifact.pack))?;
    let start = artifact.offset as usize;
    Ok(pack[start..start + artifact.bytes as usize].to_vec())
}

pub fn load_engine(bundle: &Path) -> Result<SearchEngine> {
    let manifest: SearchBundleManifest =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json"))?)
            .context("invalid manifest.json")?;
    if manifest.format != SEARCH_BUNDLE_FORMAT {
        bail!("unsupported bundle format {}", manifest.format);
    }
    let mut engine = SearchEngine::new(
        &read(bundle, &manifest, manifest.index)?,
        manifest.index.decoded_bytes,
        manifest.profile.clone(),
    )
    .map_err(anyhow::Error::msg)?;
    let tables = engine.tables().clone();
    for (index, artifact) in tables.lexicon.iter().enumerate() {
        engine.load_lexicon_block(index as u32, &read(bundle, &manifest, *artifact)?, artifact.decoded_bytes).map_err(anyhow::Error::msg)?;
    }
    for (index, artifact) in tables.documents.iter().enumerate() {
        engine.load_document_block(index as u32, &read(bundle, &manifest, *artifact)?, artifact.decoded_bytes).map_err(anyhow::Error::msg)?;
    }
    for (index, artifact) in tables.postings.iter().enumerate() {
        engine.load_postings_chunk(index as u32, &read(bundle, &manifest, *artifact)?, artifact.decoded_bytes).map_err(anyhow::Error::msg)?;
    }
    for (index, artifact) in tables.content.iter().enumerate() {
        engine.load_content_chunk(index as u32, &read(bundle, &manifest, *artifact)?, artifact.decoded_bytes).map_err(anyhow::Error::msg)?;
    }
    Ok(engine)
}

/// Write the bundle into an empty or absent directory.
pub fn write_bundle(output: &Path, bundle: &CompiledBundle) -> Result<()> {
    if output.exists() {
        fs::remove_dir_all(output)?;
    }
    fs::create_dir_all(output)?;
    for (path, bytes) in bundle.manifest.packs.iter().zip(&bundle.packs) {
        fs::write(output.join(path), bytes)?;
    }
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&bundle.manifest)?,
    )?;
    Ok(())
}
