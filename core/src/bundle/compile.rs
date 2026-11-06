use super::codec::{
    encode_content, encode_document_block, encode_index, encode_lexicon_block, encode_postings,
    BundleIndex, RankRow,
};
use super::compression::compress_artifact;
use super::model::{ArtifactRef, BlockTables, CompiledBundle, SearchBundleManifest, SEARCH_BUNDLE_FORMAT};
use crate::analysis::{analyze_document, normalize_text};
use crate::document::{DocumentMeta, IndexDocument, Posting};
use crate::profile::Profile;
use crate::query::{FilterOption, FilterOptions};
use std::collections::{BTreeMap, HashMap};

const POSTINGS_TARGET_CHUNK_BYTES: usize = 32 * 1024;
const LEXICON_TARGET_BLOCK_BYTES: usize = 32 * 1024;
const DOCUMENT_TARGET_BLOCK_BYTES: usize = 16 * 1024;
const CONTENT_TARGET_CHUNK_BYTES: usize = 32 * 1024;
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

fn lexicon_entry_bytes(previous: &str, term: &str, chunk: u32) -> usize {
    let prefix = term
        .char_indices()
        .zip(previous.char_indices())
        .take_while(|((_, left), (_, right))| left == right)
        .last()
        .map(|((index, character), _)| index + character.len_utf8())
        .unwrap_or(0);
    var_u32_bytes(prefix as u32) + 4 + term.len() - prefix + var_u32_bytes(chunk)
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

fn build_contents(documents: &[IndexDocument]) -> (Vec<Vec<(u32, String)>>, Vec<u32>) {
    let mut chunks: Vec<Vec<(u32, String)>> = Vec::new();
    let mut current = Vec::new();
    let mut current_bytes = CHUNK_HEADER_BYTES;
    let mut assignments = vec![0_u32; documents.len()];
    for (index, document) in documents.iter().enumerate() {
        let encoded_bytes = document.content.len() + 8;
        if !current.is_empty() && current_bytes + encoded_bytes > CONTENT_TARGET_CHUNK_BYTES {
            chunks.push(std::mem::take(&mut current));
            current_bytes = CHUNK_HEADER_BYTES;
        }
        assignments[index] = chunks.len() as u32;
        current.push((index as u32, document.content.clone()));
        current_bytes += encoded_bytes;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    (chunks, assignments)
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
    // Document numbers follow ids, so the last ranking tie-break needs no stored id.
    documents.sort_by(|left, right| left.id.cmp(&right.id));

    let (content_chunks, content_assignments) = build_contents(&documents);
    let metas: Vec<DocumentMeta> = documents
        .iter()
        .enumerate()
        .map(|(index, document)| DocumentMeta {
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
            content_chunk: content_assignments[index],
        })
        .collect();

    let mut postings_by_term: BTreeMap<String, Vec<Posting>> = BTreeMap::new();
    for (document_index, document) in documents.iter().enumerate() {
        let title_terms = term_counts(&document.title, 4);
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
    let mut document_blocks: Vec<Vec<DocumentMeta>> = Vec::new();
    let mut document_block_starts = Vec::new();
    let mut current_block = Vec::new();
    let mut current_bytes = 12;
    for (index, document) in metas.iter().enumerate() {
        let encoded_bytes = document_meta_bytes(document);
        if !current_block.is_empty() && current_bytes + encoded_bytes > DOCUMENT_TARGET_BLOCK_BYTES
        {
            document_block_starts.push((index - current_block.len()) as u32);
            document_blocks.push(std::mem::take(&mut current_block));
            current_bytes = 12;
        }
        current_block.push(document.clone());
        current_bytes += encoded_bytes;
    }
    if !current_block.is_empty() {
        document_block_starts.push((metas.len() - current_block.len()) as u32);
        document_blocks.push(current_block);
    }
    let mut lexicon_entries = Vec::new();
    let mut encoded_postings = Vec::with_capacity(postings_chunks.len());
    for (chunk_index, entries) in postings_chunks.iter().enumerate() {
        for (term, _) in entries {
            lexicon_entries.push((term.clone(), chunk_index as u32));
        }
        encoded_postings.push(encode_postings(entries));
    }
    let lexicon_terms = lexicon_entries.len();
    let mut lexicon_blocks: Vec<Vec<(String, u32)>> = Vec::new();
    let mut current_block = Vec::new();
    let mut current_bytes = CHUNK_HEADER_BYTES;
    let mut previous = String::new();
    for (term, chunk) in &lexicon_entries {
        let encoded_bytes = lexicon_entry_bytes(&previous, term, *chunk);
        if !current_block.is_empty() && current_bytes + encoded_bytes > LEXICON_TARGET_BLOCK_BYTES {
            lexicon_blocks.push(std::mem::take(&mut current_block));
            current_bytes = CHUNK_HEADER_BYTES;
            previous.clear();
        }
        current_block.push((term.clone(), *chunk));
        current_bytes += lexicon_entry_bytes(&previous, term, *chunk);
        previous.clone_from(term);
    }
    if !current_block.is_empty() {
        lexicon_blocks.push(current_block);
    }

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
        lexicon: lexicon_blocks.iter().map(|entries| place(encode_lexicon_block(entries))).collect(),
        postings: encoded_postings.into_iter().map(&mut place).collect(),
        documents: document_blocks.iter().enumerate().map(|(index, entries)| place(encode_document_block(document_block_starts[index], entries))).collect(),
        content: content_chunks.iter().map(|entries| place(encode_content(entries))).collect(),
    };

    let mut sources: Vec<String> = metas.iter().map(|document| document.source.clone()).collect();
    let mut categories: Vec<String> = metas.iter().filter_map(|document| document.category.clone()).collect();
    let mut dates: Vec<String> = metas.iter().filter_map(|document| document.published_at.clone()).collect();
    for list in [&mut sources, &mut categories, &mut dates] {
        list.sort();
        list.dedup();
    }
    let mut url_uses: HashMap<&str, u32> = HashMap::new();
    for document in &metas {
        *url_uses.entry(document.url.as_str()).or_default() += 1;
    }
    let url_groups: HashMap<&str, u32> = url_uses
        .into_iter()
        .filter(|(_, uses)| *uses > 1)
        .enumerate()
        .map(|(group, (url, _))| (url, group as u32 + 1))
        .collect();
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
            url_group: url_groups.get(document.url.as_str()).copied().unwrap_or(0),
            within: source.within.iter().map(|entity| position(&entities, entity)).collect(),
        })
        .collect();
    let index = BundleIndex {
        sources,
        categories,
        dates,
        entities,
        rows,
        block_starts: document_block_starts,
        filter_options,
        lexicon_firsts: lexicon_blocks.iter().map(|block| block[0].0.clone()).collect(),
        tables: tables.clone(),
    };
    let encoded = encode_index(&index);
    let compressed = compress(&encoded);
    let index_ref = ArtifactRef { pack: 0, offset: 0, bytes: compressed.len() as u64, decoded_bytes: encoded.len() as u64 };
    packs[0] = compressed;

    let manifest = SearchBundleManifest {
        format: SEARCH_BUNDLE_FORMAT.to_string(),
        index: index_ref,
        packs: (0..packs.len()).map(|pack| format!("pack-{pack:03}.bin")).collect(),
        mirrors: Vec::new(),
        profile,
    };
    Ok(CompiledBundle {
        manifest,
        packs,
        tables,
        document_count: metas.len() as u32,
        lexicon_terms,
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
