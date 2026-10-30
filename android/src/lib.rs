use std::sync::{Mutex, MutexGuard};
use wasm_search_core::{
    compile_search_bundle, CompiledBundle, IndexDocument, Profile, SearchSession,
};

uniffi::setup_scaffolding!();

#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum SearchError {
    Message { message: String },
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message { message } => formatter.write_str(message),
        }
    }
}

impl std::error::Error for SearchError {}

fn error(error: impl ToString) -> SearchError {
    SearchError::Message {
        message: error.to_string(),
    }
}

/// The same JSON search protocol as the WebAssembly binding.
/// Calls are serialized; use one engine per independent search session.
#[derive(uniffi::Object)]
pub struct SearchEngine {
    inner: Mutex<SearchSession>,
}

impl SearchEngine {
    fn session(&self) -> MutexGuard<'_, SearchSession> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

#[uniffi::export]
impl SearchEngine {
    #[uniffi::constructor]
    pub fn new(
        documents: Vec<u8>,
        document_bytes: u32,
        lexicon: Vec<u8>,
        lexicon_bytes: u32,
        profile_json: String,
    ) -> Result<Self, SearchError> {
        Ok(Self {
            inner: Mutex::new(
                SearchSession::new(
                    &documents,
                    document_bytes,
                    &lexicon,
                    lexicon_bytes,
                    &profile_json,
                )
                .map_err(error)?,
            ),
        })
    }

    pub fn load_postings_chunk(
        &self,
        chunk: u32,
        bytes: Vec<u8>,
        decoded_bytes: u32,
    ) -> Result<(), SearchError> {
        self.session()
            .load_postings_chunk(chunk, &bytes, decoded_bytes)
            .map_err(error)
    }

    pub fn load_content_chunk(
        &self,
        chunk: u32,
        bytes: Vec<u8>,
        decoded_bytes: u32,
    ) -> Result<(), SearchError> {
        self.session()
            .load_content_chunk(chunk, &bytes, decoded_bytes)
            .map_err(error)
    }

    pub fn clear_content(&self) {
        self.session().clear_content();
    }

    pub fn begin_search(&self, request_json: String) -> Result<String, SearchError> {
        self.session().begin_search(&request_json).map_err(error)
    }

    pub fn prepare_search(&self) -> Result<String, SearchError> {
        self.session().prepare_search().map_err(error)
    }

    pub fn required_content_chunks(&self, offset: u32, limit: u32) -> Result<String, SearchError> {
        self.session()
            .required_content_chunks(offset, limit)
            .map_err(error)
    }

    pub fn hydrate_search(&self, offset: u32, limit: u32) -> Result<String, SearchError> {
        self.session().hydrate_search(offset, limit).map_err(error)
    }

    pub fn filter_options(&self) -> Result<String, SearchError> {
        self.session().filter_options().map_err(error)
    }

    pub fn document_count(&self) -> u32 {
        self.session().document_count()
    }
}

/// A compiled bundle: the manifest plus its files, as in the wasm binding.
#[derive(uniffi::Object)]
pub struct Bundle {
    inner: CompiledBundle,
}

#[uniffi::export]
impl Bundle {
    pub fn manifest_json(&self) -> Result<String, SearchError> {
        serde_json::to_string_pretty(&self.inner.manifest).map_err(error)
    }

    pub fn file_count(&self) -> u32 {
        self.inner.artifacts.len() as u32
    }

    pub fn file_path(&self, index: u32) -> Result<String, SearchError> {
        self.inner
            .artifacts
            .get(index as usize)
            .map(|artifact| artifact.reference.path.clone())
            .ok_or_else(|| error("artifact index out of bounds"))
    }

    pub fn file_bytes(&self, index: u32) -> Result<Vec<u8>, SearchError> {
        self.inner
            .artifacts
            .get(index as usize)
            .map(|artifact| artifact.bytes.clone())
            .ok_or_else(|| error("artifact index out of bounds"))
    }
}

/// Compile newline-delimited JSON documents into a bundle.
#[uniffi::export]
pub fn compile(documents_jsonl: String, profile_json: String) -> Result<Bundle, SearchError> {
    let documents = documents_jsonl
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<IndexDocument>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(error)?;
    let profile: Profile = serde_json::from_str(&profile_json).map_err(error)?;
    Ok(Bundle {
        inner: compile_search_bundle(documents, profile).map_err(error)?,
    })
}

#[cfg(test)]
mod tests;
