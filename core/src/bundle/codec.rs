use super::model::{ArtifactRef, BlockTables};
use crate::document::{Attachment, DocumentKind, DocumentMeta, Posting};
use crate::query::{FilterOption, FilterOptions};

const INDEX_MAGIC: &[u8; 8] = b"WSINDX03";
const DOCUMENT_BLOCK_MAGIC: &[u8; 8] = b"WSDBK202";
const LEXICON_BLOCK_MAGIC: &[u8; 8] = b"WSLBK202";
const POSTINGS_MAGIC: &[u8; 8] = b"WSPOST01";
const CONTENT_MAGIC: &[u8; 8] = b"WSCONT01";

struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn new(magic: &[u8; 8]) -> Self {
        Self {
            bytes: magic.to_vec(),
        }
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn var_u32(&mut self, mut value: u32) {
        while value >= 0x80 {
            self.u8((value as u8 & 0x7f) | 0x80);
            value >>= 7;
        }
        self.u8(value as u8);
    }

    fn string(&mut self, value: &str) {
        self.u32(value.len() as u32);
        self.bytes.extend_from_slice(value.as_bytes());
    }

    fn optional_string(&mut self, value: Option<&str>) {
        match value {
            Some(value) => {
                self.u8(1);
                self.string(value);
            }
            None => self.u8(0),
        }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], magic: &[u8; 8]) -> Result<Self, String> {
        if bytes.get(..8) != Some(magic) {
            return Err("incompatible search artifact codec".to_string());
        }
        Ok(Self { bytes, offset: 8 })
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| "artifact offset overflow".to_string())?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| "truncated search artifact".to_string())?;
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, String> {
        let mut bytes = [0_u8; 4];
        bytes.copy_from_slice(self.take(4)?);
        Ok(u32::from_le_bytes(bytes))
    }

    fn var_u32(&mut self) -> Result<u32, String> {
        let mut value = 0_u32;
        for shift in (0..35).step_by(7) {
            let byte = self.u8()?;
            value |= ((byte & 0x7f) as u32) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("invalid varint in search artifact".to_string())
    }

    fn string(&mut self) -> Result<String, String> {
        let length = self.u32()? as usize;
        String::from_utf8(self.take(length)?.to_vec()).map_err(|error| error.to_string())
    }

    fn optional_string(&mut self) -> Result<Option<String>, String> {
        match self.u8()? {
            0 => Ok(None),
            1 => self.string().map(Some),
            _ => Err("invalid optional string flag".to_string()),
        }
    }

    fn finish(self) -> Result<(), String> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err("trailing bytes in search artifact".to_string())
        }
    }
}

fn encode_kind(kind: DocumentKind) -> u8 {
    match kind {
        DocumentKind::Page => 1,
        DocumentKind::File => 2,
        DocumentKind::Link => 3,
    }
}

fn decode_kind(value: u8) -> Result<DocumentKind, String> {
    match value {
        1 => Ok(DocumentKind::Page),
        2 => Ok(DocumentKind::File),
        3 => Ok(DocumentKind::Link),
        _ => Err("invalid document kind".to_string()),
    }
}

fn encode_document(writer: &mut Writer, document: &DocumentMeta) {
    writer.string(&document.id);
    writer.string(&document.source);
    writer.string(&document.source_name);
    writer.string(&document.url);
    writer.string(&document.title);
    writer.optional_string(document.published_at.as_deref());
    writer.optional_string(document.updated_at.as_deref());
    writer.optional_string(document.section.as_deref());
    writer.u8(encode_kind(document.kind));
    writer.optional_string(document.category.as_deref());
    writer.u32(document.content_chunk);
    writer.u32(document.attachments.len() as u32);
    for attachment in &document.attachments {
        writer.string(&attachment.url);
        writer.string(&attachment.name);
        writer.optional_string(attachment.extension.as_deref());
    }
}

fn decode_document(reader: &mut Reader<'_>) -> Result<DocumentMeta, String> {
    let id = reader.string()?;
    let source = reader.string()?;
    let source_name = reader.string()?;
    let url = reader.string()?;
    let title = reader.string()?;
    let published_at = reader.optional_string()?;
    let updated_at = reader.optional_string()?;
    let section = reader.optional_string()?;
    let kind = decode_kind(reader.u8()?)?;
    let category = reader.optional_string()?;
    let content_chunk = reader.u32()?;
    let attachment_count = reader.u32()? as usize;
    let mut attachments = Vec::with_capacity(attachment_count);
    for _ in 0..attachment_count {
        attachments.push(Attachment {
            url: reader.string()?,
            name: reader.string()?,
            extension: reader.optional_string()?,
        });
    }
    Ok(DocumentMeta {
        id,
        source,
        source_name,
        url,
        title,
        published_at,
        updated_at,
        section,
        kind,
        category,
        attachments,
        content_chunk,
    })
}

fn encode_filter_options(writer: &mut Writer, options: &FilterOptions) {
    let encode = |writer: &mut Writer, values: &[FilterOption]| {
        writer.u32(values.len() as u32);
        for value in values {
            writer.string(&value.id);
            writer.string(&value.label);
            writer.u32(value.count as u32);
        }
    };
    encode(writer, &options.sources);
    encode(writer, &options.categories);
    writer.u32(options.categories_by_source.len() as u32);
    for (source, values) in &options.categories_by_source {
        writer.string(source);
        encode(writer, values);
    }
}

fn decode_filter_options(reader: &mut Reader<'_>) -> Result<FilterOptions, String> {
    let decode = |reader: &mut Reader<'_>| -> Result<Vec<FilterOption>, String> {
        let count = reader.u32()? as usize;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(FilterOption {
                id: reader.string()?,
                label: reader.string()?,
                count: reader.u32()? as usize,
            });
        }
        Ok(values)
    };
    let sources = decode(reader)?;
    let categories = decode(reader)?;
    let source_count = reader.u32()? as usize;
    let mut categories_by_source = std::collections::BTreeMap::new();
    for _ in 0..source_count {
        categories_by_source.insert(reader.string()?, decode(reader)?);
    }
    Ok(FilterOptions {
        sources,
        categories,
        categories_by_source,
    })
}

/// What ranking needs to know about one document; everything else waits for the page.
/// `url_group` is zero for a URL used once, otherwise shared by the documents using it.
#[derive(Clone, Debug)]
pub struct RankRow {
    pub source: u32,
    pub category: u32,
    pub date: u32,
    pub kind: DocumentKind,
    pub title: String,
    pub url_group: u32,
    /// Positions in `BundleIndex::entities`.
    pub within: Vec<u32>,
}

/// The one read a client makes before its first query.
#[derive(Clone, Debug)]
pub struct BundleIndex {
    pub sources: Vec<String>,
    pub categories: Vec<String>,
    pub dates: Vec<String>,
    pub entities: Vec<String>,
    pub rows: Vec<RankRow>,
    pub block_starts: Vec<u32>,
    pub filter_options: FilterOptions,
    pub lexicon_firsts: Vec<String>,
    pub tables: BlockTables,
}

fn encode_table(writer: &mut Writer, table: &[ArtifactRef]) {
    writer.var_u32(table.len() as u32);
    for artifact in table {
        writer.var_u32(artifact.pack);
        writer.var_u32(artifact.bytes as u32);
        writer.var_u32(artifact.decoded_bytes as u32);
    }
}

/// Offsets are not stored: blocks lie end to end in each pack, in table order.
fn decode_table(reader: &mut Reader<'_>, cursor: &mut (u32, u64)) -> Result<Vec<ArtifactRef>, String> {
    let count = reader.var_u32()? as usize;
    let mut table = Vec::with_capacity(count);
    for _ in 0..count {
        let pack = reader.var_u32()?;
        if pack != cursor.0 {
            *cursor = (pack, 0);
        }
        let bytes = reader.var_u32()? as u64;
        table.push(ArtifactRef { pack, offset: cursor.1, bytes, decoded_bytes: reader.var_u32()? as u64 });
        cursor.1 += bytes;
    }
    Ok(table)
}

pub(super) fn encode_index(index: &BundleIndex) -> Vec<u8> {
    let mut writer = Writer::new(INDEX_MAGIC);
    for list in [&index.sources, &index.categories, &index.dates, &index.entities] {
        writer.var_u32(list.len() as u32);
        for value in list {
            writer.string(value);
        }
    }
    writer.var_u32(index.rows.len() as u32);
    for row in &index.rows {
        writer.var_u32(row.source);
        writer.var_u32(row.category);
        writer.var_u32(row.date);
        writer.u8(encode_kind(row.kind));
        writer.string(&row.title);
        writer.var_u32(row.url_group);
        writer.var_u32(row.within.len() as u32);
        for entity in &row.within {
            writer.var_u32(*entity);
        }
    }
    writer.var_u32(index.block_starts.len() as u32);
    let mut previous = 0;
    for start in &index.block_starts {
        writer.var_u32(start - previous);
        previous = *start;
    }
    encode_filter_options(&mut writer, &index.filter_options);
    writer.var_u32(index.lexicon_firsts.len() as u32);
    for first in &index.lexicon_firsts {
        writer.string(first);
    }
    for table in [&index.tables.lexicon, &index.tables.postings, &index.tables.documents, &index.tables.content] {
        encode_table(&mut writer, table);
    }
    writer.finish()
}

pub(crate) fn decode_index(bytes: &[u8]) -> Result<BundleIndex, String> {
    let mut reader = Reader::new(bytes, INDEX_MAGIC)?;
    let mut strings = || -> Result<Vec<String>, String> {
        let count = reader.var_u32()? as usize;
        (0..count).map(|_| reader.string()).collect()
    };
    let (sources, categories, dates, entities) = (strings()?, strings()?, strings()?, strings()?);
    let count = reader.var_u32()? as usize;
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let source = reader.var_u32()?;
        let category = reader.var_u32()?;
        let date = reader.var_u32()?;
        let kind = decode_kind(reader.u8()?)?;
        let title = reader.string()?;
        let url_group = reader.var_u32()?;
        let belongs = reader.var_u32()? as usize;
        let mut within = Vec::with_capacity(belongs);
        for _ in 0..belongs {
            within.push(reader.var_u32()?);
        }
        rows.push(RankRow { source, category, date, kind, title, url_group, within });
    }
    let count = reader.var_u32()? as usize;
    let mut block_starts = Vec::with_capacity(count);
    let mut previous = 0;
    for _ in 0..count {
        previous += reader.var_u32()?;
        block_starts.push(previous);
    }
    let filter_options = decode_filter_options(&mut reader)?;
    let count = reader.var_u32()? as usize;
    let mut lexicon_firsts = Vec::with_capacity(count);
    for _ in 0..count {
        lexicon_firsts.push(reader.string()?);
    }
    let mut cursor = (u32::MAX, 0);
    let tables = BlockTables {
        lexicon: decode_table(&mut reader, &mut cursor)?,
        postings: decode_table(&mut reader, &mut cursor)?,
        documents: decode_table(&mut reader, &mut cursor)?,
        content: decode_table(&mut reader, &mut cursor)?,
    };
    reader.finish()?;
    Ok(BundleIndex { sources, categories, dates, entities, rows, block_starts, filter_options, lexicon_firsts, tables })
}

pub(super) fn encode_document_block(start: u32, documents: &[DocumentMeta]) -> Vec<u8> {
    let mut writer = Writer::new(DOCUMENT_BLOCK_MAGIC);
    writer.u32(start);
    writer.u32(documents.len() as u32);
    for document in documents {
        encode_document(&mut writer, document);
    }
    writer.finish()
}

pub(crate) fn decode_document_block(bytes: &[u8]) -> Result<(u32, Vec<DocumentMeta>), String> {
    let mut reader = Reader::new(bytes, DOCUMENT_BLOCK_MAGIC)?;
    let start = reader.u32()?;
    let count = reader.u32()? as usize;
    let mut documents = Vec::with_capacity(count);
    for _ in 0..count {
        documents.push(decode_document(&mut reader)?);
    }
    reader.finish()?;
    Ok((start, documents))
}

pub(super) fn encode_lexicon_block(entries: &[(String, u32)]) -> Vec<u8> {
    let mut writer = Writer::new(LEXICON_BLOCK_MAGIC);
    writer.u32(entries.len() as u32);
    let mut previous = String::new();
    for (term, chunk) in entries {
        let prefix = term
            .char_indices()
            .zip(previous.char_indices())
            .take_while(|((_, left), (_, right))| left == right)
            .last()
            .map(|((index, character), _)| index + character.len_utf8())
            .unwrap_or(0);
        writer.var_u32(prefix as u32);
        writer.string(&term[prefix..]);
        writer.var_u32(*chunk);
        previous.clone_from(term);
    }
    writer.finish()
}

pub(crate) fn decode_lexicon_block(bytes: &[u8]) -> Result<Vec<(String, u32)>, String> {
    let mut reader = Reader::new(bytes, LEXICON_BLOCK_MAGIC)?;
    let count = reader.u32()? as usize;
    let mut entries = Vec::with_capacity(count);
    let mut previous = String::new();
    for _ in 0..count {
        let prefix = reader.var_u32()? as usize;
        if prefix > previous.len() || !previous.is_char_boundary(prefix) {
            return Err("invalid lexicon prefix".to_string());
        }
        let term = format!("{}{}", &previous[..prefix], reader.string()?);
        let chunk = reader.var_u32()?;
        previous.clone_from(&term);
        entries.push((term, chunk));
    }
    reader.finish()?;
    Ok(entries)
}

pub(super) fn encode_postings(entries: &[(String, Vec<Posting>)]) -> Vec<u8> {
    let mut writer = Writer::new(POSTINGS_MAGIC);
    writer.u32(entries.len() as u32);
    for (term, postings) in entries {
        writer.string(term);
        writer.u32(postings.len() as u32);
        let mut previous_document = 0_u32;
        for posting in postings {
            writer.var_u32(posting.document - previous_document);
            writer.u8(posting.title_hits.min(u8::MAX as u16) as u8);
            writer.u8(posting.body_hits.min(u8::MAX as u16) as u8);
            previous_document = posting.document;
        }
    }
    writer.finish()
}

pub(crate) fn decode_postings(bytes: &[u8]) -> Result<Vec<(String, Vec<Posting>)>, String> {
    let mut reader = Reader::new(bytes, POSTINGS_MAGIC)?;
    let count = reader.u32()? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let term = reader.string()?;
        let posting_count = reader.u32()? as usize;
        let mut postings = Vec::with_capacity(posting_count);
        let mut previous_document = 0_u32;
        for _ in 0..posting_count {
            let document = previous_document
                .checked_add(reader.var_u32()?)
                .ok_or_else(|| "posting document id overflow".to_string())?;
            postings.push(Posting {
                document,
                title_hits: reader.u8()? as u16,
                body_hits: reader.u8()? as u16,
            });
            previous_document = document;
        }
        entries.push((term, postings));
    }
    reader.finish()?;
    Ok(entries)
}

pub(super) fn encode_content(entries: &[(u32, String)]) -> Vec<u8> {
    let mut writer = Writer::new(CONTENT_MAGIC);
    writer.u32(entries.len() as u32);
    for (document, content) in entries {
        writer.u32(*document);
        writer.string(content);
    }
    writer.finish()
}

pub(crate) fn decode_content(bytes: &[u8]) -> Result<Vec<(u32, String)>, String> {
    let mut reader = Reader::new(bytes, CONTENT_MAGIC)?;
    let count = reader.u32()? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        entries.push((reader.u32()?, reader.string()?));
    }
    reader.finish()?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::{decode_lexicon_block, encode_lexicon_block};

    #[test]
    fn decoder_rejects_trailing_bytes() {
        let mut bytes = encode_lexicon_block(&[("示例".to_string(), 0)]);
        bytes.push(0);
        assert_eq!(
            decode_lexicon_block(&bytes).unwrap_err(),
            "trailing bytes in search artifact"
        );
    }
}
