use wasm_bindgen::prelude::*;
use wasm_search_core::{
    compile_search_bundle_with, CompiledBundle, IndexDocument, Profile, SearchSession,
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
    pub fn new(index: &[u8], index_bytes: u32, profile_json: &str) -> Result<SearchEngine, JsValue> {
        Ok(Self { inner: SearchSession::new(index, index_bytes, profile_json).map_err(js_error)? })
    }

    pub fn load_lexicon_block(
        &mut self,
        block: u32,
        bytes: &[u8],
        decoded_bytes: u32,
    ) -> Result<(), JsValue> {
        self.inner
            .load_lexicon_block(block, bytes, decoded_bytes)
            .map_err(js_error)
    }

    pub fn load_document_block(
        &mut self,
        block: u32,
        bytes: &[u8],
        decoded_bytes: u32,
    ) -> Result<(), JsValue> {
        self.inner
            .load_document_block(block, bytes, decoded_bytes)
            .map_err(js_error)
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

    pub fn required_posting_chunks(&self) -> Result<String, JsValue> {
        self.inner.required_posting_chunks().map_err(js_error)
    }

    pub fn plan_search(&mut self) -> Result<String, JsValue> {
        self.inner.plan_search().map_err(js_error)
    }

    pub fn ranked_results(&self) -> Result<String, JsValue> {
        self.inner.ranked_results().map_err(js_error)
    }

    pub fn required_content_chunks(&self) -> Result<String, JsValue> {
        self.inner.required_content_chunks().map_err(js_error)
    }

    pub fn hydrate_search(&self) -> Result<String, JsValue> {
        self.inner.hydrate_search().map_err(js_error)
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
        self.inner.packs.len() as u32
    }

    pub fn file_path(&self, index: u32) -> String {
        self.inner.manifest.packs[index as usize].clone()
    }

    pub fn file_bytes(&self, index: u32) -> Vec<u8> {
        self.inner.packs[index as usize].clone()
    }
}

/// Compile newline-delimited JSON documents into a bundle.
#[wasm_bindgen]
pub fn compile(documents_jsonl: &str, profile_json: &str, compress: &js_sys::Function) -> Result<Bundle, JsValue> {
    let documents = documents_jsonl
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<IndexDocument>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(js_error)?;
    let profile: Profile = serde_json::from_str(profile_json).map_err(js_error)?;
    Ok(Bundle {
        inner: compile_search_bundle_with(documents, profile, &mut |bytes| {
            let input = js_sys::Uint8Array::from(bytes);
            let output = compress.call1(&JsValue::NULL, &input).expect("compressor");
            js_sys::Uint8Array::new(&output).to_vec()
        })
        .map_err(js_error)?,
    })
}
