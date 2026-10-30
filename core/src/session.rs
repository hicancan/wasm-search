use crate::{Profile, Query, QueryPlan, QueryPreparation, SearchEngine};

fn message(error: impl ToString) -> String {
    error.to_string()
}

/// Shared JSON protocol and query lifecycle for native and WebAssembly clients.
pub struct SearchSession {
    inner: SearchEngine,
    preparation: Option<QueryPreparation>,
    plan: Option<QueryPlan>,
    plan_request: Option<Query>,
    page_limit: usize,
}

impl SearchSession {
    pub fn new(
        documents: &[u8],
        document_bytes: u32,
        lexicon: &[u8],
        lexicon_bytes: u32,
        profile_json: &str,
    ) -> Result<SearchSession, String> {
        let profile: Profile = serde_json::from_str(profile_json).map_err(message)?;
        Ok(Self {
            inner: SearchEngine::new(
                documents,
                document_bytes.into(),
                lexicon,
                lexicon_bytes.into(),
                profile,
            )
            .map_err(message)?,
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
    ) -> Result<(), String> {
        self.inner
            .load_postings_chunk(chunk, bytes, decoded_bytes.into())
            .map_err(message)
    }

    pub fn load_content_chunk(
        &mut self,
        chunk: u32,
        bytes: &[u8],
        decoded_bytes: u32,
    ) -> Result<(), String> {
        self.inner
            .load_content_chunk(chunk, bytes, decoded_bytes.into())
            .map_err(message)
    }

    pub fn clear_content(&mut self) {
        self.inner.clear_content();
    }

    pub fn begin_search(&mut self, request_json: &str) -> Result<String, String> {
        let request: Query = serde_json::from_str(request_json).map_err(message)?;
        self.page_limit = request.limit;
        let same_query = self.plan_request.as_ref().is_some_and(|previous| {
            previous.query == request.query
                && previous.sort == request.sort
                && previous.filters == request.filters
        });
        if same_query && self.plan.is_some() {
            return serde_json::to_string(&Vec::<u32>::new()).map_err(message);
        }
        let preparation = self.inner.begin_query(&request).map_err(message)?;
        let chunks = self.inner.required_posting_chunks(&preparation);
        self.preparation = Some(preparation);
        self.plan = None;
        self.plan_request = Some(request);
        serde_json::to_string(&chunks).map_err(message)
    }

    pub fn prepare_search(&mut self) -> Result<String, String> {
        if self.plan.is_none() {
            self.plan = Some(
                self.inner
                    .plan_query(
                        self.preparation
                            .as_ref()
                            .ok_or_else(|| message("query is not prepared"))?,
                    )
                    .map_err(message)?,
            );
        }
        let response = self
            .inner
            .result_shells(
                self.plan
                    .as_ref()
                    .ok_or_else(|| message("query is not prepared"))?,
                0,
                self.page_limit,
            )
            .map_err(message)?;
        serde_json::to_string(&response).map_err(message)
    }

    pub fn required_content_chunks(&self, offset: u32, limit: u32) -> Result<String, String> {
        let plan = self
            .plan
            .as_ref()
            .ok_or_else(|| message("query is not prepared"))?;
        serde_json::to_string(&self.inner.required_content_chunks(
            plan,
            offset as usize,
            limit as usize,
        ))
        .map_err(message)
    }

    pub fn hydrate_search(&self, offset: u32, limit: u32) -> Result<String, String> {
        let plan = self
            .plan
            .as_ref()
            .ok_or_else(|| message("query is not prepared"))?;
        let response = self
            .inner
            .hydrate_results(plan, offset as usize, limit as usize)
            .map_err(message)?;
        serde_json::to_string(&response).map_err(message)
    }

    pub fn filter_options(&self) -> Result<String, String> {
        serde_json::to_string(&self.inner.filter_options()).map_err(message)
    }

    pub fn document_count(&self) -> u32 {
        self.inner.document_count() as u32
    }
}
