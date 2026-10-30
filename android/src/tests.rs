use super::*;
use serde_json::{json, Value};
use wasm_search_core::{ArtifactRef, Query, SearchEngine as CoreSearchEngine};

const DOCUMENTS: &str = r#"
{"id":"a","source":"admissions","source_name":"Admissions","url":"https://example.test/a","title":"转专业通知","text":"转专业申请材料和办理流程","category":"notice","published":"2026-07-01","files":[{"name":"申请表.pdf","url":"https://example.test/a.pdf","ext":"pdf"}]}
{"id":"b","source":"news","source_name":"News","url":"https://example.test/b","title":"转专业安排","text":"本年度转专业安排","category":"notice","published":"2026-08-01"}
{"id":"c","source":"news","source_name":"News","url":"https://example.test/c","title":"校园活动","text":"其他校园新闻","category":"news"}
{"id":"d","source":"admissions","source_name":"Admissions","url":"https://example.test/d","title":"转专业指南","text":"转专业常见问题","category":"guide"}
"#;
const PROFILE: &str =
    r#"{"aliases":[{"terms":["transfer"],"phrases":["转专业"],"prefer_sources":["admissions"]}]}"#;

fn fixture() -> CompiledBundle {
    compile_search_bundle(
        DOCUMENTS
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect(),
        serde_json::from_str(PROFILE).unwrap(),
    )
    .unwrap()
}

fn bytes(bundle: &CompiledBundle, reference: &ArtifactRef) -> Vec<u8> {
    bundle
        .artifacts
        .iter()
        .find(|artifact| artifact.reference.path == reference.path)
        .unwrap()
        .bytes
        .clone()
}

fn decoded_bytes(reference: &ArtifactRef) -> u32 {
    reference.decoded_bytes.try_into().unwrap()
}

fn engine(bundle: &CompiledBundle) -> SearchEngine {
    SearchEngine::new(
        bytes(bundle, &bundle.manifest.documents),
        decoded_bytes(&bundle.manifest.documents),
        bytes(bundle, &bundle.manifest.lexicon),
        decoded_bytes(&bundle.manifest.lexicon),
        PROFILE.into(),
    )
    .unwrap()
}

fn value(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

fn chunks(json: &str) -> Vec<u32> {
    serde_json::from_str(json).unwrap()
}

#[test]
fn staged_search_matches_core_including_filters_paging_and_content_reload() {
    let bundle = fixture();
    let manifest = &bundle.manifest;
    let android = engine(&bundle);
    let mut core = CoreSearchEngine::new(
        &bytes(&bundle, &manifest.documents),
        manifest.documents.decoded_bytes,
        &bytes(&bundle, &manifest.lexicon),
        manifest.lexicon.decoded_bytes,
        manifest.profile.clone(),
    )
    .unwrap();
    for (chunk, reference) in manifest.postings.iter().enumerate() {
        core.load_postings_chunk(
            chunk as u32,
            &bytes(&bundle, reference),
            reference.decoded_bytes,
        )
        .unwrap();
    }
    for (chunk, reference) in manifest.content.iter().enumerate() {
        core.load_content_chunk(
            chunk as u32,
            &bytes(&bundle, reference),
            reference.decoded_bytes,
        )
        .unwrap();
    }
    assert_eq!(android.document_count(), 4);
    assert_eq!(
        value(&android.filter_options().unwrap()),
        serde_json::to_value(core.filter_options()).unwrap()
    );

    let requests = [
        json!({"query":"transfer","limit":1,"filters":{"category":"notice"}}),
        // Same plan, larger page: no postings should be requested again.
        json!({"query":"transfer","limit":10,"filters":{"category":"notice"}}),
        json!({"query":"转专业","sort":"date_desc","filters":{"excludedSourceIds":["news"],"includeUndated":true}}),
        json!({"query":"转专业","filters":{"sourceId":"news","publishedFrom":"2026-07-01","publishedTo":"2026-08-31"}}),
    ];
    for (index, request) in requests.iter().enumerate() {
        let posting_chunks = chunks(&android.begin_search(request.to_string()).unwrap());
        if index == 0 {
            assert!(!posting_chunks.is_empty());
        } else if index == 1 {
            assert!(posting_chunks.is_empty());
        }
        for chunk in posting_chunks {
            let reference = &manifest.postings[chunk as usize];
            android
                .load_postings_chunk(chunk, bytes(&bundle, reference), decoded_bytes(reference))
                .unwrap();
        }
        let query: Query = serde_json::from_value(request.clone()).unwrap();
        let preparation = core.begin_query(&query).unwrap();
        let plan = core.plan_query(&preparation).unwrap();
        let shells = value(&android.prepare_search().unwrap());
        assert_eq!(
            shells,
            serde_json::to_value(core.result_shells(&plan, 0, query.limit).unwrap()).unwrap()
        );
        if index == 0 {
            assert_eq!(shells["results"][0]["id"], "a");
            assert_eq!(shells["totalCandidates"], 2);
        }
        android.clear_content();
        let content_chunks = chunks(
            &android
                .required_content_chunks(0, query.limit as u32)
                .unwrap(),
        );
        assert!(!content_chunks.is_empty());
        for chunk in content_chunks {
            let reference = &manifest.content[chunk as usize];
            android
                .load_content_chunk(chunk, bytes(&bundle, reference), decoded_bytes(reference))
                .unwrap();
        }
        assert_eq!(
            android
                .required_content_chunks(0, query.limit as u32)
                .unwrap(),
            "[]"
        );
        assert_eq!(
            value(&android.hydrate_search(0, query.limit as u32).unwrap()),
            serde_json::to_value(core.search(&query).unwrap()).unwrap()
        );
        assert_eq!(
            value(&android.hydrate_search(1, 1).unwrap()),
            serde_json::to_value(core.hydrate_results(&plan, 1, 1).unwrap()).unwrap()
        );
    }
}

#[test]
fn errors_preserve_messages_through_one_uniffi_type() {
    let android = engine(&fixture());
    for result in [
        android.prepare_search(),
        android.required_content_chunks(0, 10),
        android.hydrate_search(0, 10),
    ] {
        let SearchError::Message { message } = result.unwrap_err();
        assert_eq!(message, "query is not prepared");
    }
    assert!(android.begin_search("not JSON".into()).is_err());
    assert!(android
        .begin_search(r#"{"query":"transfer","filters":{"sourceId":"all"}}"#.into())
        .unwrap_err()
        .to_string()
        .contains("real source"));
    assert!(android.load_postings_chunk(0, vec![], 0).is_err());
    assert!(android.load_content_chunk(0, vec![], 0).is_err());
    assert!(SearchEngine::new(vec![], 0, vec![], 0, "invalid".into()).is_err());
    assert!(SearchEngine::new(vec![], 0, vec![], 0, PROFILE.into()).is_err());
}

#[test]
fn compile_mirrors_the_wasm_bundle_helpers() {
    let expected = fixture();
    let bundle = compile(DOCUMENTS.into(), PROFILE.into()).unwrap();
    assert_eq!(
        value(&bundle.manifest_json().unwrap()),
        serde_json::to_value(&expected.manifest).unwrap()
    );
    assert_eq!(bundle.file_count() as usize, expected.artifacts.len());
    for (index, artifact) in expected.artifacts.iter().enumerate() {
        assert_eq!(
            bundle.file_path(index as u32).unwrap(),
            artifact.reference.path
        );
        assert_eq!(bundle.file_bytes(index as u32).unwrap(), artifact.bytes);
    }
    assert!(bundle.file_path(bundle.file_count()).is_err());
    assert!(bundle.file_bytes(bundle.file_count()).is_err());
    assert!(compile("invalid".into(), PROFILE.into()).is_err());
    assert!(compile(DOCUMENTS.into(), "invalid".into()).is_err());
}
