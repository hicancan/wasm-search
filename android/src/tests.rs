use super::*;
use serde_json::{json, Value};
use wasm_search_core::{ArtifactRef, BlockRef, Query, SearchEngine as CoreSearchEngine};

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

fn bytes(bundle: &CompiledBundle, reference: ArtifactRef) -> Vec<u8> {
    bundle.read(reference).to_vec()
}

fn engine(bundle: &CompiledBundle) -> SearchEngine {
    SearchEngine::new(
        bytes(bundle, bundle.manifest.index),
        bundle.manifest.index.decoded_bytes.try_into().unwrap(),
        PROFILE.into(),
    )
    .unwrap()
}

fn value(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

fn blocks(json: &str) -> Vec<BlockRef> {
    serde_json::from_str(json).unwrap()
}

#[test]
fn staged_search_matches_core_including_filters_paging_and_content_reload() {
    let bundle = fixture();
    let android = engine(&bundle);
    let mut core = CoreSearchEngine::new(
        &bytes(&bundle, bundle.manifest.index),
        bundle.manifest.index.decoded_bytes,
        bundle.manifest.profile.clone(),
    )
    .unwrap();
    let tables = &bundle.tables;
    for (id, reference) in tables.lexicon.iter().enumerate() {
        core.load_lexicon_block(id as u32, &bytes(&bundle, *reference), reference.decoded_bytes).unwrap();
    }
    for (id, reference) in tables.documents.iter().enumerate() {
        core.load_document_block(id as u32, &bytes(&bundle, *reference), reference.decoded_bytes).unwrap();
    }
    for (id, reference) in tables.postings.iter().enumerate() {
        core.load_postings_chunk(id as u32, &bytes(&bundle, *reference), reference.decoded_bytes).unwrap();
    }
    for (id, reference) in tables.content.iter().enumerate() {
        core.load_content_chunk(id as u32, &bytes(&bundle, *reference), reference.decoded_bytes).unwrap();
    }
    assert_eq!(android.document_count(), 4);
    assert_eq!(value(&android.filter_options().unwrap()), serde_json::to_value(core.filter_options()).unwrap());

    let requests = [
        json!({"query":"transfer","limit":1,"filters":{"category":"notice"}}),
        // Same plan, larger page: no postings should be requested again.
        json!({"query":"transfer","limit":10,"filters":{"category":"notice"}}),
        json!({"query":"转专业","sort":"date_desc","filters":{"excludedSourceIds":["news"],"includeUndated":true}}),
        json!({"query":"转专业","filters":{"sourceIds":["news"],"publishedFrom":"2026-07-01","publishedTo":"2026-12-31"}}),
    ];
    for (index, request) in requests.iter().enumerate() {
        for block in blocks(&android.begin_search(request.to_string()).unwrap()) {
            android.load_lexicon_block(block.id, bytes(&bundle, tables.lexicon[block.id as usize]), block.decoded_bytes as u32).unwrap();
        }
        let posting_blocks = blocks(&android.required_posting_chunks().unwrap());
        if index == 0 {
            assert!(!posting_blocks.is_empty());
        } else if index == 1 {
            assert!(posting_blocks.is_empty());
        }
        for block in posting_blocks {
            android.load_postings_chunk(block.id, bytes(&bundle, tables.postings[block.id as usize]), block.decoded_bytes as u32).unwrap();
        }
        for block in blocks(&android.plan_search().unwrap()) {
            android.load_document_block(block.id, bytes(&bundle, tables.documents[block.id as usize]), block.decoded_bytes as u32).unwrap();
        }
        let query: Query = serde_json::from_value(request.clone()).unwrap();
        let preparation = core.begin_query(&query).unwrap();
        let plan = core.plan_query(&preparation).unwrap();
        let shells = value(&android.ranked_results().unwrap());
        assert_eq!(shells, serde_json::to_value(core.result_shells(&plan, 0, query.limit).unwrap()).unwrap());
        if index == 0 {
            assert_eq!(shells["results"][0]["id"], "a");
            assert_eq!(shells["totalCandidates"], 2);
        }
        android.clear_content();
        let content = blocks(&android.required_content_chunks().unwrap());
        assert!(!content.is_empty());
        for block in content {
            android.load_content_chunk(block.id, bytes(&bundle, tables.content[block.id as usize]), block.decoded_bytes as u32).unwrap();
        }
        assert_eq!(android.required_content_chunks().unwrap(), "[]");
        assert_eq!(value(&android.hydrate_search().unwrap()), serde_json::to_value(core.search(&query).unwrap()).unwrap());
    }
}

#[test]
fn errors_preserve_messages_through_one_uniffi_type() {
    let android = engine(&fixture());
    let SearchError::Message { message } = android.plan_search().unwrap_err();
    assert_eq!(message, "query is not prepared");
    for result in [android.ranked_results(), android.required_content_chunks(), android.hydrate_search()] {
        let SearchError::Message { message } = result.unwrap_err();
        assert_eq!(message, "query is not planned");
    }
    assert!(android.begin_search("not JSON".into()).is_err());
    assert!(android
        .begin_search(r#"{"query":"transfer","filters":{"sourceIds":["all"]}}"#.into())
        .unwrap_err()
        .to_string()
        .contains("real source"));
    assert!(android.load_postings_chunk(0, vec![], 0).is_err());
    assert!(android.load_content_chunk(0, vec![], 0).is_err());
    assert!(SearchEngine::new(vec![], 0, "invalid".into()).is_err());
    assert!(SearchEngine::new(vec![], 0, PROFILE.into()).is_err());
}

#[test]
fn compile_mirrors_the_wasm_bundle_helpers() {
    let expected = fixture();
    let bundle = compile(DOCUMENTS.into(), PROFILE.into()).unwrap();
    assert_eq!(
        value(&bundle.manifest_json().unwrap()),
        serde_json::to_value(&expected.manifest).unwrap()
    );
    assert_eq!(bundle.file_count() as usize, expected.packs.len());
    for (index, pack) in expected.packs.iter().enumerate() {
        assert_eq!(bundle.file_path(index as u32).unwrap(), expected.manifest.packs[index]);
        assert_eq!(&bundle.file_bytes(index as u32).unwrap(), pack);
    }
    assert!(bundle.file_path(bundle.file_count()).is_err());
    assert!(bundle.file_bytes(bundle.file_count()).is_err());
    assert!(compile("invalid".into(), PROFILE.into()).is_err());
    assert!(compile(DOCUMENTS.into(), "invalid".into()).is_err());
}

#[test]
fn pinyin_uses_the_same_compiled_postings_through_android() {
    let documents = r#"
{"id":"library","source":"places","source_name":"Places","url":"https://example.test/library","title":"图书馆","text":"借阅与学习"}
{"id":"classroom","source":"places","source_name":"Places","url":"https://example.test/classroom","title":"教3","text":"教学"}
{"id":"literal","source":"places","source_name":"Places","url":"https://example.test/literal","title":"tsg","text":"Latin title"}
"#;
    let bundle = compile_search_bundle(
        documents
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect(),
        Default::default(),
    )
    .unwrap();
    let android = engine(&bundle);
    for (query, expected) in [
        ("tushuguan", "library"),
        ("tu shu guan", "library"),
        ("TSG", "literal"),
        ("jiao3", "classroom"),
        ("j3", "classroom"),
        ("图书馆", "library"),
    ] {
        let tables = &bundle.tables;
        for block in blocks(&android.begin_search(json!({"query":query}).to_string()).unwrap()) {
            android.load_lexicon_block(block.id, bytes(&bundle, tables.lexicon[block.id as usize]), block.decoded_bytes as u32).unwrap();
        }
        for block in blocks(&android.required_posting_chunks().unwrap()) {
            android.load_postings_chunk(block.id, bytes(&bundle, tables.postings[block.id as usize]), block.decoded_bytes as u32).unwrap();
        }
        for block in blocks(&android.plan_search().unwrap()) {
            android.load_document_block(block.id, bytes(&bundle, tables.documents[block.id as usize]), block.decoded_bytes as u32).unwrap();
        }
        let result = value(&android.ranked_results().unwrap());
        assert_eq!(result["results"][0]["id"], expected, "{query}");
        if query == "TSG" {
            assert_eq!(result["results"][1]["id"], "library");
        }
        for block in blocks(&android.required_content_chunks().unwrap()) {
            android.load_content_chunk(block.id, bytes(&bundle, tables.content[block.id as usize]), block.decoded_bytes as u32).unwrap();
        }
        let hydrated = value(&android.hydrate_search().unwrap());
        assert_eq!(hydrated["results"][0]["id"], expected);
        assert!(!hydrated.to_string().contains("py:"));
    }
}
