use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use wasm_search_core::{
    ArtifactRef, CompiledBundle, SearchBundleManifest, SearchEngine, SEARCH_BUNDLE_FORMAT,
};

fn read(bundle: &Path, artifact: &ArtifactRef) -> Result<Vec<u8>> {
    fs::read(bundle.join(&artifact.path))
        .with_context(|| format!("missing bundle file {}", artifact.path))
}

pub fn load_engine(bundle: &Path) -> Result<SearchEngine> {
    let manifest: SearchBundleManifest =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json"))?)
            .context("invalid manifest.json")?;
    if manifest.format != SEARCH_BUNDLE_FORMAT {
        bail!("unsupported bundle format {}", manifest.format);
    }
    let mut engine = SearchEngine::new(
        &read(bundle, &manifest.documents)?,
        manifest.documents.decoded_bytes,
        &read(bundle, &manifest.lexicon)?,
        manifest.lexicon.decoded_bytes,
        manifest.profile.clone(),
    )
    .map_err(anyhow::Error::msg)?;
    for (index, artifact) in manifest.postings.iter().enumerate() {
        engine
            .load_postings_chunk(
                index as u32,
                &read(bundle, artifact)?,
                artifact.decoded_bytes,
            )
            .map_err(anyhow::Error::msg)?;
    }
    for (index, artifact) in manifest.content.iter().enumerate() {
        engine
            .load_content_chunk(
                index as u32,
                &read(bundle, artifact)?,
                artifact.decoded_bytes,
            )
            .map_err(anyhow::Error::msg)?;
    }
    Ok(engine)
}

/// Write the bundle into an empty or absent directory.
pub fn write_bundle(output: &Path, bundle: &CompiledBundle) -> Result<()> {
    if output.exists() {
        fs::remove_dir_all(output)?;
    }
    fs::create_dir_all(output)?;
    for artifact in &bundle.artifacts {
        fs::write(output.join(&artifact.reference.path), &artifact.bytes)?;
    }
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&bundle.manifest)?,
    )?;
    Ok(())
}
