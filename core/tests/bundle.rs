use std::collections::BTreeSet;
use wasm_search_core::{
    compile_search_bundle, Attachment, CompiledBundle, DocumentKind, IndexDocument, Profile, Query,
    SearchEngine, SortMode,
};

fn documents() -> Vec<IndexDocument> {
    vec![
        IndexDocument {
            id: "doc-a".to_string(),
            source: "jwc".to_string(),
            source_name: "教务处".to_string(),
            url: "https://example.test/a".to_string(),
            title: "转专业办理通知".to_string(),
            content: "转专业 转专业 申请材料与办理流程".to_string(),
            published_at: Some("2026-07-01".to_string()),
            updated_at: None,
            section: Some("通知公告".to_string()),
            kind: DocumentKind::Page,
            category: None,
            tags: vec!["本科生".to_string()],
            attachments: vec![Attachment {
                url: "https://example.test/a.pdf".to_string(),
                name: "申请表.pdf".to_string(),
                extension: Some("pdf".to_string()),
            }],
        },
        IndexDocument {
            id: "doc-b".to_string(),
            source: "www".to_string(),
            source_name: "南邮主页".to_string(),
            url: "https://example.test/b".to_string(),
            title: "校园新闻".to_string(),
            content: "学校新闻动态".to_string(),
            published_at: Some("2025-01-01".to_string()),
            updated_at: None,
            section: Some("新闻".to_string()),
            kind: DocumentKind::Page,
            category: None,
            tags: vec![],
            attachments: vec![],
        },
        IndexDocument {
            id: "doc-c".to_string(),
            source: "jwc".to_string(),
            source_name: "教务处".to_string(),
            url: "https://example.test/c".to_string(),
            title: "专业建设实施方案".to_string(),
            content: "专业 专业 专业 专业 专业 专业 专业 专业".to_string(),
            published_at: Some("2026-07-02".to_string()),
            updated_at: None,
            section: Some("政策".to_string()),
            kind: DocumentKind::Page,
            category: None,
            tags: vec![],
            attachments: vec![],
        },
    ]
}

fn categorized(mut values: Vec<IndexDocument>) -> Vec<IndexDocument> {
    for value in &mut values {
        value.category = Some(
            if value.title.contains("考试") {
                "exam"
            } else {
                "notice"
            }
            .to_string(),
        );
    }
    values
}

fn artifact<'a>(bundle: &'a CompiledBundle, path: &str) -> &'a [u8] {
    &bundle
        .artifacts
        .iter()
        .find(|artifact| artifact.reference.path == path)
        .expect("artifact")
        .bytes
}

fn engine(bundle: &CompiledBundle) -> SearchEngine {
    let mut engine = SearchEngine::new(
        artifact(bundle, "documents.bin"),
        bundle.manifest.documents.decoded_bytes,
        artifact(bundle, "lexicon.bin"),
        bundle.manifest.lexicon.decoded_bytes,
        bundle.manifest.profile.clone(),
    )
    .expect("metadata");
    for (index, reference) in bundle.manifest.postings.iter().enumerate() {
        engine
            .load_postings_chunk(
                index as u32,
                artifact(bundle, &reference.path),
                reference.decoded_bytes,
            )
            .expect("postings");
    }
    for (index, reference) in bundle.manifest.content.iter().enumerate() {
        engine
            .load_content_chunk(
                index as u32,
                artifact(bundle, &reference.path),
                reference.decoded_bytes,
            )
            .expect("content");
    }
    engine
}

fn candidate_contract_documents() -> Vec<IndexDocument> {
    let mut values = Vec::new();
    for index in 0..5 {
        values.push(IndexDocument {
            id: format!("title-{index}"),
            source: "source-a".to_string(),
            source_name: "来源 A".to_string(),
            url: format!("https://a.example.test/{index}"),
            title: format!("肖甫参加第{index}场校务活动"),
            content: "活动报道".to_string(),
            published_at: Some(format!("2026-07-{:02}", index + 1)),
            updated_at: None,
            section: Some("新闻".to_string()),
            kind: if index == 4 {
                DocumentKind::Link
            } else {
                DocumentKind::Page
            },
            category: None,
            tags: vec![],
            attachments: vec![],
        });
    }
    for index in 0..9 {
        values.push(IndexDocument {
            id: format!("body-{index}"),
            source: "source-b".to_string(),
            source_name: "来源 B".to_string(),
            url: format!("https://b.example.test/{index}"),
            title: if index < 3 {
                format!("第{index}场考试工作动态")
            } else {
                format!("第{index}场学院工作动态")
            },
            content: "会议由肖甫主持并作总结".to_string(),
            published_at: Some(format!("2026-08-{:02}", index + 1)),
            updated_at: None,
            section: Some("通知公告".to_string()),
            kind: DocumentKind::Page,
            category: None,
            tags: vec![],
            attachments: vec![],
        });
    }
    values
}

fn request(query: &str) -> Query {
    Query {
        query: query.to_string(),
        limit: 100,
        sort: SortMode::Relevance,
        filters: Default::default(),
    }
}

fn urls(response: &wasm_search_core::SearchResponse) -> BTreeSet<&str> {
    response
        .results
        .iter()
        .map(|result| result.url.as_str())
        .collect()
}

fn presentation_identity(result: &wasm_search_core::SearchResult) -> String {
    let title = result
        .title
        .strip_prefix('【')
        .and_then(|rest| rest.find('】').map(|end| &rest[end + '】'.len_utf8()..]))
        .unwrap_or(&result.title)
        .trim();
    format!(
        "{}\u{1f}{title}",
        result.published_at.as_deref().unwrap_or("")
    )
}

#[test]
fn compiles_and_queries_the_current_bundle_contract() {
    let bundle =
        compile_search_bundle(categorized(documents()), Profile::default()).expect("compile");
    let engine = engine(&bundle);
    let response = engine
        .search(&Query {
            query: "转专业".to_string(),
            limit: 10,
            sort: Default::default(),
            filters: Default::default(),
        })
        .expect("search");
    assert_eq!(response.total_candidates, 2);
    assert_eq!(response.results[0].id, "doc-a");
    assert_eq!(response.results[0].attachments[0].name, "申请表.pdf");
}

#[test]
fn filter_options_keep_category_counts_scoped_to_each_source() {
    let bundle =
        compile_search_bundle(categorized(documents()), Profile::default()).expect("compile");
    let options = engine(&bundle).filter_options();

    let jwc = options
        .categories_by_source
        .get("jwc")
        .expect("jwc categories");
    let www = options
        .categories_by_source
        .get("www")
        .expect("www categories");
    assert_eq!(jwc.iter().map(|facet| facet.count).sum::<usize>(), 2);
    assert_eq!(www.iter().map(|facet| facet.count).sum::<usize>(), 1);
    assert_eq!(
        options
            .categories
            .iter()
            .map(|facet| facet.count)
            .sum::<usize>(),
        3
    );

    let json = serde_json::to_value(&options).expect("filter options JSON");
    assert!(json.get("categoriesBySource").is_some());
    assert!(json.get("categories_by_source").is_none());
}

#[test]
fn one_query_plan_keeps_exact_order_while_snippets_are_hydrated() {
    let bundle =
        compile_search_bundle(categorized(documents()), Profile::default()).expect("compile");
    let mut engine = engine(&bundle);
    let request = Query {
        query: "转专业".to_string(),
        limit: 10,
        sort: Default::default(),
        filters: Default::default(),
    };
    let preparation = engine.begin_query(&request).expect("analysis");
    let plan = engine.plan_query(&preparation).expect("ranking");
    let shells = engine.result_shells(&plan, 0, 10).expect("shells");
    assert!(shells.results.iter().all(|result| result.snippet.is_none()));

    engine.clear_content();
    let chunks = engine.required_content_chunks(&plan, 0, 10);
    for chunk in chunks {
        let reference = &bundle.manifest.content[chunk as usize];
        engine
            .load_content_chunk(
                chunk,
                artifact(&bundle, &reference.path),
                reference.decoded_bytes,
            )
            .expect("content");
    }
    let hydrated = engine.hydrate_results(&plan, 0, 10).expect("hydrated");
    assert_eq!(
        shells
            .results
            .iter()
            .map(|result| &result.id)
            .collect::<Vec<_>>(),
        hydrated
            .results
            .iter()
            .map(|result| &result.id)
            .collect::<Vec<_>>()
    );
    assert!(hydrated
        .results
        .iter()
        .all(|result| result.snippet.is_some()));
}

#[test]
fn rejects_ui_sentinels_and_invalid_dates() {
    let bundle =
        compile_search_bundle(categorized(documents()), Profile::default()).expect("compile");
    let engine = engine(&bundle);
    let mut query = Query {
        query: "转专业".to_string(),
        limit: 10,
        sort: Default::default(),
        filters: Default::default(),
    };
    query.filters.source_id = Some("all".to_string());
    assert!(engine.search(&query).unwrap_err().contains("real source"));
    query.filters.source_id = None;
    query.filters.excluded_source_ids = vec!["source-a".to_string(), "source-a".to_string()];
    assert!(engine.search(&query).unwrap_err().contains("duplicates"));
    query.filters.excluded_source_ids = vec!["source-a".to_string()];
    query.filters.source_id = Some("source-a".to_string());
    assert!(engine
        .search(&query)
        .unwrap_err()
        .contains("also be excluded"));
    query.filters.source_id = None;
    query.filters.excluded_source_ids.clear();
    query.filters.published_from = Some("2026-02-30".to_string());
    assert!(engine.search(&query).unwrap_err().contains("invalid date"));
}

#[test]
fn excluded_sources_keep_product_scopes_disjoint() {
    let bundle = compile_search_bundle(
        categorized(candidate_contract_documents()),
        Profile::default(),
    )
    .expect("compile");
    let engine = engine(&bundle);
    let all = engine.search(&request("肖甫")).expect("all sources");

    let mut scoped_request = request("肖甫");
    scoped_request.filters.excluded_source_ids = vec!["source-b".to_string()];
    let scoped = engine
        .search(&scoped_request)
        .expect("scope without source B");
    assert!(urls(&scoped).is_subset(&urls(&all)));
    assert!(scoped
        .results
        .iter()
        .all(|result| result.source != "source-b"));
    assert!(scoped.total_candidates < all.total_candidates);
}

#[test]
fn result_page_does_not_repeat_same_day_department_reposts() {
    let mut values = documents();
    values.push(IndexDocument {
        id: "contest-jwc".to_string(),
        source: "jwc".to_string(),
        source_name: "教务处".to_string(),
        url: "https://jwc.example.test/contest".to_string(),
        title: "【实践科】2026年全国大学生数学建模竞赛报名通知".to_string(),
        content: "数学建模竞赛报名".to_string(),
        published_at: Some("2026-05-25".to_string()),
        updated_at: None,
        section: Some("通知公告".to_string()),
        kind: DocumentKind::Page,
        category: None,
        tags: vec![],
        attachments: vec![],
    });
    values.push(IndexDocument {
        id: "contest-cxcy".to_string(),
        source: "cxcy".to_string(),
        source_name: "创新创业教育学院".to_string(),
        url: "https://cxcy.example.test/contest".to_string(),
        title: "2026年全国大学生数学建模竞赛报名通知".to_string(),
        content: "数学建模竞赛报名".to_string(),
        published_at: Some("2026-05-25".to_string()),
        updated_at: None,
        section: Some("通知公告".to_string()),
        kind: DocumentKind::Page,
        category: None,
        tags: vec![],
        attachments: vec![],
    });
    let bundle = compile_search_bundle(values, Profile::default()).expect("compile");
    let response = engine(&bundle)
        .search(&Query {
            query: "数学建模竞赛报名".to_string(),
            limit: 10,
            sort: Default::default(),
            filters: Default::default(),
        })
        .expect("search");
    assert_eq!(
        response
            .results
            .iter()
            .filter(|result| result
                .title
                .contains("2026年全国大学生数学建模竞赛报名通知"))
            .count(),
        1
    );
}

#[test]
fn complete_body_matches_survive_title_matches_and_source_filtering_is_monotone() {
    let bundle = compile_search_bundle(
        categorized(candidate_contract_documents()),
        Profile::default(),
    )
    .expect("compile");
    let engine = engine(&bundle);
    let all = engine.search(&request("肖甫")).expect("all sources");
    assert_eq!(all.total_candidates, 14);

    let mut source_request = request("肖甫");
    source_request.filters.source_id = Some("source-b".to_string());
    let source = engine.search(&source_request).expect("source B");
    assert_eq!(source.total_candidates, 9);
    assert!(urls(&source).is_subset(&urls(&all)));
    assert!(all.results[0].title.contains("肖甫"));
    assert_eq!(
        source
            .results
            .iter()
            .filter(|result| result.title.contains("肖甫"))
            .count(),
        0
    );
}

#[test]
fn date_facet_and_sort_only_narrow_or_reorder_the_stable_candidate_set() {
    let bundle = compile_search_bundle(
        categorized(candidate_contract_documents()),
        Profile::default(),
    )
    .expect("compile");
    let engine = engine(&bundle);
    let all = engine.search(&request("肖甫")).expect("all");

    let mut dated_request = request("肖甫");
    dated_request.filters.published_from = Some("2026-08-01".to_string());
    let dated = engine.search(&dated_request).expect("date filter");
    assert!(urls(&dated).is_subset(&urls(&all)));

    let mut facet_request = request("肖甫");
    facet_request.filters.category = Some("exam".to_string());
    let faceted = engine.search(&facet_request).expect("facet filter");
    assert_eq!(faceted.total_candidates, 3);
    assert!(urls(&faceted).is_subset(&urls(&all)));

    let mut date_sort_request = request("肖甫");
    date_sort_request.sort = SortMode::DateDesc;
    let date_sorted = engine.search(&date_sort_request).expect("date sort");
    assert_eq!(date_sorted.total_candidates, all.total_candidates);
    assert_eq!(urls(&date_sorted), urls(&all));
    assert_ne!(
        date_sorted
            .results
            .iter()
            .map(|result| &result.id)
            .collect::<Vec<_>>(),
        all.results
            .iter()
            .map(|result| &result.id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn filters_cannot_reenable_partial_fallback_candidates() {
    let mut values = Vec::new();
    for index in 0..20 {
        values.push(IndexDocument {
            id: format!("complete-{index}"),
            source: "complete".to_string(),
            source_name: "完整匹配来源".to_string(),
            url: format!("https://complete.example.test/{index}"),
            title: format!("第{index}项工作通知"),
            content: "项目申报工作".to_string(),
            published_at: Some("2025-01-01".to_string()),
            updated_at: None,
            section: Some("通知公告".to_string()),
            kind: DocumentKind::Page,
            category: None,
            tags: vec![],
            attachments: vec![],
        });
    }
    for index in 0..5 {
        values.push(IndexDocument {
            id: format!("partial-{index}"),
            source: "partial".to_string(),
            source_name: "局部匹配来源".to_string(),
            url: format!("https://partial.example.test/{index}"),
            title: format!("第{index}项考试安排"),
            content: "项目进展".to_string(),
            published_at: Some("2026-01-01".to_string()),
            updated_at: None,
            section: Some("考试通知".to_string()),
            kind: DocumentKind::Page,
            category: None,
            tags: vec![],
            attachments: vec![],
        });
    }
    let bundle = compile_search_bundle(values, Profile::default()).expect("compile");
    let engine = engine(&bundle);
    let all = engine.search(&request("项目申报")).expect("all");
    assert_eq!(all.total_candidates, 20);

    let mut source_request = request("项目申报");
    source_request.filters.source_id = Some("partial".to_string());
    assert_eq!(
        engine
            .search(&source_request)
            .expect("source")
            .total_candidates,
        0
    );

    let mut date_request = request("项目申报");
    date_request.filters.published_from = Some("2026-01-01".to_string());
    assert_eq!(
        engine.search(&date_request).expect("date").total_candidates,
        0
    );

    let mut facet_request = request("项目申报");
    facet_request.filters.category = Some("exam".to_string());
    assert_eq!(
        engine
            .search(&facet_request)
            .expect("facet")
            .total_candidates,
        0
    );
}

#[test]
fn canonical_reposts_preserve_filter_monotonicity_after_deduplication() {
    let values = vec![
        IndexDocument {
            id: "repost-a".to_string(),
            source: "source-a".to_string(),
            source_name: "来源 A".to_string(),
            url: "https://a.example.test/repost".to_string(),
            title: "【实践科】肖甫竞赛报名通知".to_string(),
            content: "肖甫竞赛报名通知".to_string(),
            published_at: Some("2026-05-25".to_string()),
            updated_at: None,
            section: Some("通知公告".to_string()),
            kind: DocumentKind::Page,
            category: None,
            tags: vec![],
            attachments: vec![],
        },
        IndexDocument {
            id: "repost-b".to_string(),
            source: "source-b".to_string(),
            source_name: "来源 B".to_string(),
            url: "https://b.example.test/repost".to_string(),
            title: "肖甫竞赛报名通知".to_string(),
            content: "肖甫竞赛报名通知".to_string(),
            published_at: Some("2026-05-25".to_string()),
            updated_at: None,
            section: Some("通知公告".to_string()),
            kind: DocumentKind::Page,
            category: None,
            tags: vec![],
            attachments: vec![],
        },
    ];
    let bundle = compile_search_bundle(values, Profile::default()).expect("compile");
    let engine = engine(&bundle);
    let all = engine.search(&request("肖甫")).expect("all");
    assert_eq!(all.total_candidates, 1);

    let mut source_request = request("肖甫");
    source_request.filters.source_id = Some("source-b".to_string());
    let source = engine.search(&source_request).expect("source B");
    assert_eq!(source.total_candidates, 1);
    let all_keys = all
        .results
        .iter()
        .map(presentation_identity)
        .collect::<BTreeSet<_>>();
    let source_keys = source
        .results
        .iter()
        .map(presentation_identity)
        .collect::<BTreeSet<_>>();
    assert!(source_keys.is_subset(&all_keys));
}

#[test]
fn query_plan_can_page_beyond_the_old_one_hundred_result_ui_cap() {
    let values = (0..101)
        .map(|index| IndexDocument {
            id: format!("page-{index}"),
            source: "source".to_string(),
            source_name: "来源".to_string(),
            url: format!("https://page.example.test/{index}"),
            title: format!("第{index}项肖甫工作动态"),
            content: "肖甫参加活动".to_string(),
            published_at: Some("2026-01-01".to_string()),
            updated_at: None,
            section: Some("新闻".to_string()),
            kind: DocumentKind::Page,
            category: None,
            tags: vec![],
            attachments: vec![],
        })
        .collect();
    let bundle = compile_search_bundle(values, Profile::default()).expect("compile");
    let response = engine(&bundle)
        .search(&Query {
            query: "肖甫".to_string(),
            limit: 101,
            sort: SortMode::Relevance,
            filters: Default::default(),
        })
        .expect("search");
    assert_eq!(response.total_candidates, 101);
    assert_eq!(response.results.len(), 101);
}
