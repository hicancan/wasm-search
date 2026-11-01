use wasm_bindgen::prelude::*;
use wasm_search_core::{
    compile_search_bundle, CompiledBundle, IndexDocument, Profile, SearchSession,
};

fn js_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[wasm_bindgen]
pub struct SearchEngine {
    inner: SearchSession,
}

#[wasm_bindgen]
impl SearchEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(
        documents: &[u8],
        document_bytes: u32,
        lexicon: &[u8],
        lexicon_bytes: u32,
        profile_json: &str,
    ) -> Result<SearchEngine, JsValue> {
        Ok(Self {
            inner: SearchSession::new(
                documents,
                document_bytes,
                lexicon,
                lexicon_bytes,
                profile_json,
            )
            .map_err(js_error)?,
        })
    }

    pub fn load_postings_chunk(
        &mut self,
        chunk: u32,
        bytes: &[u8],
        decoded_bytes: u32,
    ) -> Result<(), JsValue> {
        self.inner
            .load_postings_chunk(chunk, bytes, decoded_bytes)
            .map_err(js_error)
    }

    pub fn load_content_chunk(
        &mut self,
        chunk: u32,
        bytes: &[u8],
        decoded_bytes: u32,
    ) -> Result<(), JsValue> {
        self.inner
            .load_content_chunk(chunk, bytes, decoded_bytes)
            .map_err(js_error)
    }

    pub fn clear_content(&mut self) {
        self.inner.clear_content();
    }

    pub fn begin_search(&mut self, request_json: &str) -> Result<String, JsValue> {
        self.inner.begin_search(request_json).map_err(js_error)
    }

    pub fn prepare_search(&mut self) -> Result<String, JsValue> {
        self.inner.prepare_search().map_err(js_error)
    }

    pub fn required_content_chunks(&self, offset: u32, limit: u32) -> Result<String, JsValue> {
        self.inner
            .required_content_chunks(offset, limit)
            .map_err(js_error)
    }

    pub fn hydrate_search(&self, offset: u32, limit: u32) -> Result<String, JsValue> {
        self.inner.hydrate_search(offset, limit).map_err(js_error)
    }

    pub fn filter_options(&self) -> Result<String, JsValue> {
        self.inner.filter_options().map_err(js_error)
    }

    pub fn document_count(&self) -> u32 {
        self.inner.document_count()
    }
}

/// A compiled bundle handed to JavaScript: the manifest plus its files.
#[wasm_bindgen]
pub struct Bundle {
    inner: CompiledBundle,
}

#[wasm_bindgen]
impl Bundle {
    pub fn manifest_json(&self) -> Result<String, JsValue> {
        serde_json::to_string_pretty(&self.inner.manifest).map_err(js_error)
    }

    pub fn file_count(&self) -> u32 {
        self.inner.artifacts.len() as u32
    }

    pub fn file_path(&self, index: u32) -> String {
        self.inner.artifacts[index as usize].reference.path.clone()
    }

    pub fn file_bytes(&self, index: u32) -> Vec<u8> {
        self.inner.artifacts[index as usize].bytes.clone()
    }
}

/// Compile newline-delimited JSON documents into a bundle.
#[wasm_bindgen]
pub fn compile(documents_jsonl: &str, profile_json: &str) -> Result<Bundle, JsValue> {
    let documents = documents_jsonl
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<IndexDocument>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(js_error)?;
    let profile: Profile = serde_json::from_str(profile_json).map_err(js_error)?;
    Ok(Bundle {
        inner: compile_search_bundle(documents, profile).map_err(js_error)?,
    })
}
