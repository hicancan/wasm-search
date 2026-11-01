use super::codec::{
    encode_content, encode_document_block, encode_document_index, encode_lexicon_block,
    encode_lexicon_index, encode_postings,
};
use super::compression::compress_artifact;
use super::model::{
    ArtifactRef, CompiledArtifact, CompiledBundle, SearchBundleManifest, SEARCH_BUNDLE_FORMAT,
};
use crate::analysis::analyze_document;
use crate::document::{DocumentMeta, IndexDocument, Posting};
use crate::profile::Profile;
use crate::query::{FilterOption, FilterOptions};
use std::collections::{BTreeMap, HashMap};

const POSTINGS_TARGET_CHUNK_BYTES: usize = 128 * 1024;
const LEXICON_TARGET_BLOCK_BYTES: usize = 128 * 1024;
const DOCUMENT_TARGET_BLOCK_BYTES: usize = 16 * 1024;
const CONTENT_TARGET_CHUNK_BYTES: usize = 128 * 1024;
const CHUNK_HEADER_BYTES: usize = 12;

fn compiled_artifact(path: String, decoded: Vec<u8>) -> CompiledArtifact {
    let decoded_bytes = decoded.len() as u64;
    let bytes = compress_artifact(&decoded);
    let reference = ArtifactRef {
        path,
        bytes: bytes.len() as u64,
        decoded_bytes,
    };
    CompiledArtifact { reference, bytes }
}

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
    if documents.is_empty() {
        return Err("cannot build a search bundle from no documents".to_string());
    }

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
    let document_index_artifact = compiled_artifact(
        "documents-index.bin".to_string(),
        encode_document_index(metas.len() as u32, &document_block_starts, &filter_options),
    );
    let mut artifacts = vec![document_index_artifact];
    let mut lexicon_entries = Vec::new();
    let mut posting_refs = Vec::new();
    for (chunk_index, entries) in postings_chunks.iter().enumerate() {
        for (term, _) in entries {
            lexicon_entries.push((term.clone(), chunk_index as u32));
        }
        let artifact = compiled_artifact(
            format!("postings-{chunk_index:04}.bin"),
            encode_postings(entries),
        );
        posting_refs.push(artifact.reference.clone());
        artifacts.push(artifact);
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
    let lexicon_index: Vec<_> = lexicon_blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block[0].0.clone(), index as u32))
        .collect();
    let lexicon_index_artifact = compiled_artifact(
        "lexicon-index.bin".to_string(),
        encode_lexicon_index(&lexicon_index),
    );
    let lexicon_index_ref = lexicon_index_artifact.reference.clone();
    artifacts.insert(1, lexicon_index_artifact);
    let mut lexicon_block_refs = Vec::new();
    for (index, entries) in lexicon_blocks.iter().enumerate() {
        let artifact = compiled_artifact(
            format!("lexicon-{index:04}.bin"),
            encode_lexicon_block(entries),
        );
        lexicon_block_refs.push(artifact.reference.clone());
        artifacts.push(artifact);
    }

    let mut content_refs = Vec::new();
    for (chunk_index, entries) in content_chunks.iter().enumerate() {
        let artifact = compiled_artifact(
            format!("content-{chunk_index:04}.bin"),
            encode_content(entries),
        );
        content_refs.push(artifact.reference.clone());
        artifacts.push(artifact);
    }

    let mut document_block_refs = Vec::new();
    for (index, entries) in document_blocks.iter().enumerate() {
        let start = document_block_starts[index];
        let artifact = compiled_artifact(
            format!("documents-{index:04}.bin"),
            encode_document_block(start, entries),
        );
        document_block_refs.push(artifact.reference.clone());
        artifacts.push(artifact);
    }

    let manifest = SearchBundleManifest {
        format: SEARCH_BUNDLE_FORMAT.to_string(),
        documents: super::model::DocumentManifest {
            index: artifacts[0].reference.clone(),
            blocks: document_block_refs,
        },
        lexicon: super::model::LexiconManifest {
            index: lexicon_index_ref,
            blocks: lexicon_block_refs,
        },
        postings: posting_refs,
        content: content_refs,
        profile,
    };
    Ok(CompiledBundle {
        manifest,
        artifacts,
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
