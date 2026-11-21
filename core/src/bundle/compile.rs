use super::codec::{encode_index, encode_page, encode_postings, BundleIndex, RankRow};
use super::compression::compress_artifact;
use super::model::{ArtifactRef, BlockTables, CompiledBundle, SearchBundleManifest, SEARCH_BUNDLE_FORMAT};
use crate::analysis::{analyze_document, normalize_text};
use crate::document::{DocumentMeta, IndexDocument, Posting};
use crate::profile::Profile;
use crate::query::{FilterOption, FilterOptions};
use std::collections::{BTreeMap, HashMap};

const POSTINGS_TARGET_CHUNK_BYTES: usize = 32 * 1024;
const PAGE_TARGET_BYTES: usize = 32 * 1024;
const PACK_TARGET_BYTES: usize = 16 * 1024 * 1024;
const CHUNK_HEADER_BYTES: usize = 12;

fn term_counts(text: &str, max_n: usize) -> HashMap<String, u16> {
    let mut counts = HashMap::new();
    for token in analyze_document(text, max_n) {
        let count = counts.entry(token).or_insert(0_u16);
        *count = count.saturating_add(1);
    }
    for token in crate::pinyin::document_terms(text, max_n) {
        counts.insert(token, 1);
    }
    counts
}

fn var_u32_bytes(mut value: u32) -> usize {
    let mut bytes = 1;
    while value >= 0x80 {
        value >>= 7;
        bytes += 1;
    }
    bytes
}

fn posting_entry_bytes(term: &str, postings: &[Posting]) -> usize {
    let mut bytes = 4 + term.len() + 4;
    let mut previous_document = 0_u32;
    for posting in postings {
        bytes += var_u32_bytes(posting.document - previous_document) + 2;
        previous_document = posting.document;
    }
    bytes
}

fn document_filter_options(documents: &[DocumentMeta]) -> FilterOptions {
    let mut sources: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut categories: BTreeMap<String, usize> = BTreeMap::new();
    let mut categories_by_source: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for document in documents {
        *sources
            .entry((document.source.clone(), document.source_name.clone()))
            .or_default() += 1;
        if let Some(category) = &document.category {
            *categories.entry(category.clone()).or_default() += 1;
            *categories_by_source
                .entry(document.source.clone())
                .or_default()
                .entry(category.clone())
                .or_default() += 1;
        }
    }
    let options = |counts: BTreeMap<String, usize>| -> Vec<FilterOption> {
        counts
            .into_iter()
            .map(|(id, count)| FilterOption {
                label: id.clone(),
                id,
                count,
            })
            .collect()
    };
    FilterOptions {
        sources: sources
            .into_iter()
            .map(|((id, label), count)| FilterOption { id, label, count })
            .collect(),
        categories: options(categories),
        categories_by_source: categories_by_source
            .into_iter()
            .map(|(source, counts)| (source, options(counts)))
            .collect(),
    }
}

fn document_meta_bytes(document: &DocumentMeta) -> usize {
    let mut bytes = 4 + document.id.len();
    bytes += 4 + document.source.len();
    bytes += 4 + document.source_name.len();
    bytes += 4 + document.url.len();
    bytes += 4 + document.title.len();
    bytes += 1 + document.published_at.as_ref().map_or(0, |v| 4 + v.len());
    bytes += 1 + document.updated_at.as_ref().map_or(0, |v| 4 + v.len());
    bytes += 1 + document.section.as_ref().map_or(0, |v| 4 + v.len());
    bytes += 1 + 1 + document.category.as_ref().map_or(0, |v| 4 + v.len());
    bytes += 4 + 4;
    for attachment in &document.attachments {
        bytes += 4 + attachment.url.len();
        bytes += 4 + attachment.name.len();
        bytes += 1 + attachment.extension.as_ref().map_or(0, |v| 4 + v.len());
    }
    bytes
}

pub fn compile_search_bundle(
    documents: Vec<IndexDocument>,
    profile: Profile,
) -> Result<CompiledBundle, String> {
    compile_search_bundle_with(documents, profile, &mut compress_artifact)
}

/// Compiles with the caller's compressor; any zstd frame decodes. A host with a slower,
/// tighter encoder than this crate's own can pay once here to save on every fetch.
pub fn compile_search_bundle_with(
    mut documents: Vec<IndexDocument>,
    profile: Profile,
    compress: &mut dyn FnMut(&[u8]) -> Vec<u8>,
) -> Result<CompiledBundle, String> {
    if documents.is_empty() {
        return Err("cannot build a search bundle from no documents".to_string());
    }
    // Physical document numbers are local to the segment.
    documents.sort_by(|left, right| left.id.cmp(&right.id));

    let metas: Vec<DocumentMeta> = documents
        .iter()
        .map(|document| DocumentMeta {
            id: document.id.clone(),
            source: document.source.clone(),
            source_name: document.source_name.clone(),
            url: document.url.clone(),
            title: document.title.clone(),
            published_at: document.published_at.clone(),
            updated_at: document.updated_at.clone(),
            section: document.section.clone(),
            kind: document.kind,
            category: document.category.clone(),
            attachments: document.attachments.clone(),
        })
        .collect();

    let mut postings_by_term: BTreeMap<String, Vec<Posting>> = BTreeMap::new();
    for (document_index, document) in documents.iter().enumerate() {
        let mut title_terms = term_counts(&document.title, 4);
        for term in crate::pinyin::whole_terms(&document.title) {
            title_terms.insert(term, 1);
        }
        let searchable_body = format!(
            "{} {} {} {}",
            document.content,
            document.section.as_deref().unwrap_or(""),
            document.source_name,
            document.tags.join(" ")
        );
        let body_terms = term_counts(&searchable_body, 2);
        let mut all_terms: BTreeMap<String, (u16, u16)> = BTreeMap::new();
        for (term, count) in title_terms {
            all_terms.entry(term).or_default().0 = count;
        }
        for (term, count) in body_terms {
            all_terms.entry(term).or_default().1 = count;
        }
        for (term, (title_hits, body_hits)) in all_terms {
            postings_by_term.entry(term).or_default().push(Posting {
                document: document_index as u32,
                title_hits,
                body_hits,
            });
        }
    }

    let mut postings_chunks: Vec<Vec<(String, Vec<Posting>)>> = Vec::new();
    let mut current = Vec::new();
    let mut current_bytes = CHUNK_HEADER_BYTES;
    for (term, postings) in postings_by_term {
        let encoded_bytes = posting_entry_bytes(&term, &postings);
        if !current.is_empty() && current_bytes + encoded_bytes > POSTINGS_TARGET_CHUNK_BYTES {
            postings_chunks.push(std::mem::take(&mut current));
            current_bytes = CHUNK_HEADER_BYTES;
        }
        current.push((term, postings));
        current_bytes += encoded_bytes;
    }
    if !current.is_empty() {
        postings_chunks.push(current);
    }

    let filter_options = document_filter_options(&metas);
    // Pages hold runs of documents in document order, each document with its text.
    let mut pages: Vec<Vec<usize>> = Vec::new();
    let mut page_starts = Vec::new();
    let mut current_page = Vec::new();
    let mut current_bytes = CHUNK_HEADER_BYTES;
    for (index, document) in metas.iter().enumerate() {
        let encoded_bytes = document_meta_bytes(document) + 4 + documents[index].content.len();
        if !current_page.is_empty() && current_bytes + encoded_bytes > PAGE_TARGET_BYTES {
            pages.push(std::mem::take(&mut current_page));
            current_bytes = CHUNK_HEADER_BYTES;
        }
        if current_page.is_empty() {
            page_starts.push(index as u32);
        }
        current_page.push(index);
        current_bytes += encoded_bytes;
    }
    if !current_page.is_empty() {
        pages.push(current_page);
    }
    let terms = postings_chunks.iter().map(Vec::len).sum();
    let postings_firsts: Vec<String> = postings_chunks.iter().map(|entries| entries[0].0.clone()).collect();

    // Pack zero holds the index alone; blocks follow, grouped by kind.
    let mut packs: Vec<Vec<u8>> = vec![Vec::new()];
    let mut place = |decoded: Vec<u8>| {
        let compressed = compress(&decoded);
        let pack = packs.last().expect("a pack");
        if packs.len() == 1 || pack.len() + compressed.len() > PACK_TARGET_BYTES {
            packs.push(Vec::new());
        }
        let index = packs.len() - 1;
        let offset = packs[index].len() as u64;
        packs[index].extend_from_slice(&compressed);
        ArtifactRef { pack: index as u32, offset, bytes: compressed.len() as u64, decoded_bytes: decoded.len() as u64 }
    };
    let tables = BlockTables {
        postings: postings_chunks.iter().map(|entries| place(encode_postings(entries))).collect(),
        pages: pages
            .iter()
            .map(|members| {
                let entries: Vec<(&DocumentMeta, &str)> =
                    members.iter().map(|&index| (&metas[index], documents[index].content.as_str())).collect();
                place(encode_page(members[0] as u32, &entries))
            })
            .collect(),
    };

    let mut sources: Vec<String> = metas.iter().map(|document| document.source.clone()).collect();
    let mut categories: Vec<String> = metas.iter().filter_map(|document| document.category.clone()).collect();
    let mut dates: Vec<String> = metas.iter().filter_map(|document| document.published_at.clone()).collect();
    for list in [&mut sources, &mut categories, &mut dates] {
        list.sort();
        list.dedup();
    }
    // A category or date of zero means none, so dates compare as they sort.
    let position = |list: &[String], value: &str| list.binary_search_by(|item| item.as_str().cmp(value)).expect("listed") as u32;
    let mut entities: Vec<String> = documents.iter().flat_map(|document| document.within.iter().cloned()).collect();
    entities.sort();
    entities.dedup();
    let rows = metas
        .iter()
        .zip(documents)
        .map(|(document, source)| RankRow {
            source: position(&sources, &document.source),
            category: document.category.as_deref().map_or(0, |value| position(&categories, value) + 1),
            date: document.published_at.as_deref().map_or(0, |value| position(&dates, value) + 1),
            kind: document.kind,
            title: normalize_text(&document.title),
            id: document.id.clone(),
            url: document.url.clone(),
            within: source.within.iter().map(|entity| position(&entities, entity)).collect(),
        })
        .collect();
    let index = BundleIndex {
        sources,
        categories,
        dates,
        entities,
        rows,
        page_starts,
        filter_options,
        postings_firsts,
        tables: tables.clone(),
    };
    let encoded = encode_index(&index);
    let compressed = compress(&encoded);
    let index_ref = ArtifactRef { pack: 0, offset: 0, bytes: compressed.len() as u64, decoded_bytes: encoded.len() as u64 };
    packs[0] = compressed;

    let manifest = SearchBundleManifest {
        format: SEARCH_BUNDLE_FORMAT.to_string(),
        index: index_ref,
        segments: Vec::new(),
        packs: (0..packs.len()).map(|pack| format!("pack-{pack:03}.bin")).collect(),
        mirrors: Vec::new(),
        profile,
    };
    Ok(CompiledBundle {
        manifest,
        packs,
        tables,
        document_count: metas.len() as u32,
        terms,
    })
}

#[cfg(test)]
mod tests {
    use super::term_counts;

    #[test]
    fn document_analysis_preserves_term_frequency() {
        let counts = term_counts("奖学金 奖学金 奖学金", 4);
        assert_eq!(counts.get("奖学金"), Some(&3));
    }
}
