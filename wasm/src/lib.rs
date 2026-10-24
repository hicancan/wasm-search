use wasm_bindgen::prelude::*;
use wasm_search_core::{
    compile_search_bundle, CompiledBundle, IndexDocument, Profile, Query, QueryPlan,
    QueryPreparation, SearchEngine as CoreSearchEngine,
};

fn js_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[wasm_bindgen]
pub struct SearchEngine {
    inner: CoreSearchEngine,
    preparation: Option<QueryPreparation>,
    plan: Option<QueryPlan>,
    plan_request: Option<Query>,
    page_limit: usize,
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
        let profile: Profile = serde_json::from_str(profile_json).map_err(js_error)?;
        Ok(Self {
            inner: CoreSearchEngine::new(
                documents,
                document_bytes.into(),
                lexicon,
                lexicon_bytes.into(),
                profile,
            )
            .map_err(js_error)?,
            preparation: None,
            plan: None,
            plan_request: None,
            page_limit: 0,
        })
    }

    pub fn load_postings_chunk(
        &mut self,
        chunk: u32,
        bytes: &[u8],
        decoded_bytes: u32,
    ) -> Result<(), JsValue> {
        self.inner
            .load_postings_chunk(chunk, bytes, decoded_bytes.into())
            .map_err(js_error)
    }

    pub fn load_content_chunk(
        &mut self,
        chunk: u32,
        bytes: &[u8],
        decoded_bytes: u32,
    ) -> Result<(), JsValue> {
        self.inner
            .load_content_chunk(chunk, bytes, decoded_bytes.into())
            .map_err(js_error)
    }

    pub fn clear_content(&mut self) {
        self.inner.clear_content();
    }

    pub fn begin_search(&mut self, request_json: &str) -> Result<String, JsValue> {
        let request: Query = serde_json::from_str(request_json).map_err(js_error)?;
        self.page_limit = request.limit;
        let same_query = self.plan_request.as_ref().is_some_and(|previous| {
            previous.query == request.query
                && previous.sort == request.sort
                && previous.filters == request.filters
        });
        if same_query && self.plan.is_some() {
            return serde_json::to_string(&Vec::<u32>::new()).map_err(js_error);
        }
        let preparation = self.inner.begin_query(&request).map_err(js_error)?;
        let chunks = self.inner.required_posting_chunks(&preparation);
        self.preparation = Some(preparation);
        self.plan = None;
        self.plan_request = Some(request);
        serde_json::to_string(&chunks).map_err(js_error)
    }

    pub fn prepare_search(&mut self) -> Result<String, JsValue> {
        if self.plan.is_none() {
            self.plan = Some(
                self.inner
                    .plan_query(
                        self.preparation
                            .as_ref()
                            .ok_or_else(|| js_error("query is not prepared"))?,
                    )
                    .map_err(js_error)?,
            );
        }
        let response = self
            .inner
            .result_shells(
                self.plan
                    .as_ref()
                    .ok_or_else(|| js_error("query is not prepared"))?,
                0,
                self.page_limit,
            )
            .map_err(js_error)?;
        serde_json::to_string(&response).map_err(js_error)
    }

    pub fn required_content_chunks(&self, offset: u32, limit: u32) -> Result<String, JsValue> {
        let plan = self
            .plan
            .as_ref()
            .ok_or_else(|| js_error("query is not prepared"))?;
        serde_json::to_string(&self.inner.required_content_chunks(
            plan,
            offset as usize,
            limit as usize,
        ))
        .map_err(js_error)
    }

    pub fn hydrate_search(&self, offset: u32, limit: u32) -> Result<String, JsValue> {
        let plan = self
            .plan
            .as_ref()
            .ok_or_else(|| js_error("query is not prepared"))?;
        let response = self
            .inner
            .hydrate_results(plan, offset as usize, limit as usize)
            .map_err(js_error)?;
        serde_json::to_string(&response).map_err(js_error)
    }

    pub fn filter_options(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.inner.filter_options()).map_err(js_error)
    }

    pub fn document_count(&self) -> u32 {
        self.inner.document_count() as u32
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
