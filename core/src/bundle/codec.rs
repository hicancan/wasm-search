use super::model::{ArtifactRef, BlockTables};
use crate::document::{Attachment, DocumentKind, DocumentMeta, Posting};
use crate::query::{FilterOption, FilterOptions};

const INDEX_MAGIC: &[u8; 8] = b"WSINDX06";
const PAGE_MAGIC: &[u8; 8] = b"WSPAGE05";
const POSTINGS_MAGIC: &[u8; 8] = b"WSPOST01";

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
#[derive(Clone, Debug)]
pub struct RankRow {
    pub source: u32,
    pub category: u32,
    pub date: u32,
    pub kind: DocumentKind,
    pub title: String,
    pub id: String,
    pub url: String,
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
    /// The first document of each page, ascending.
    pub page_starts: Vec<u32>,
    pub filter_options: FilterOptions,
    /// The first term of each postings chunk, ascending: a term's chunk is found here, so a
    /// query reads its postings without a lexicon round first.
    pub postings_firsts: Vec<String>,
    pub tables: BlockTables,
}

/// Sorted terms, each written as the length it shares with the one before and the rest.
fn encode_sorted_terms(writer: &mut Writer, terms: &[String]) {
    writer.var_u32(terms.len() as u32);
    let mut previous = "";
    for term in terms {
        let shared = term
            .char_indices()
            .zip(previous.char_indices())
            .take_while(|((_, left), (_, right))| left == right)
            .last()
            .map(|((index, character), _)| index + character.len_utf8())
            .unwrap_or(0);
        writer.var_u32(shared as u32);
        writer.string(&term[shared..]);
        previous = term;
    }
}

fn decode_sorted_terms(reader: &mut Reader<'_>) -> Result<Vec<String>, String> {
    let count = reader.var_u32()? as usize;
    let mut terms: Vec<String> = Vec::with_capacity(count);
    for _ in 0..count {
        let shared = reader.var_u32()? as usize;
        let previous = terms.last().map(String::as_str).unwrap_or("");
        if shared > previous.len() || !previous.is_char_boundary(shared) {
            return Err("invalid shared term prefix".to_string());
        }
        let term = format!("{}{}", &previous[..shared], reader.string()?);
        terms.push(term);
    }
    Ok(terms)
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
        writer.string(&row.id);
        writer.string(&row.url);
        writer.var_u32(row.within.len() as u32);
        for entity in &row.within {
            writer.var_u32(*entity);
        }
    }
    writer.var_u32(index.page_starts.len() as u32);
    let mut previous = 0;
    for start in &index.page_starts {
        writer.var_u32(start - previous);
        previous = *start;
    }
    encode_filter_options(&mut writer, &index.filter_options);
    encode_sorted_terms(&mut writer, &index.postings_firsts);
    for table in [&index.tables.postings, &index.tables.pages] {
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
        let id = reader.string()?;
        let url = reader.string()?;
        let belongs = reader.var_u32()? as usize;
        let mut within = Vec::with_capacity(belongs);
        for _ in 0..belongs {
            within.push(reader.var_u32()?);
        }
        rows.push(RankRow { source, category, date, kind, title, id, url, within });
    }
    let count = reader.var_u32()? as usize;
    let mut page_starts = Vec::with_capacity(count);
    let mut previous = 0;
    for _ in 0..count {
        previous += reader.var_u32()?;
        page_starts.push(previous);
    }
    let filter_options = decode_filter_options(&mut reader)?;
    let postings_firsts = decode_sorted_terms(&mut reader)?;
    let mut cursor = (u32::MAX, 0);
    let tables = BlockTables {
        postings: decode_table(&mut reader, &mut cursor)?,
        pages: decode_table(&mut reader, &mut cursor)?,
    };
    reader.finish()?;
    if tables.postings.len() != postings_firsts.len() || tables.pages.len() != page_starts.len() {
        return Err("index tables do not match their keys".to_string());
    }
    Ok(BundleIndex { sources, categories, dates, entities, rows, page_starts, filter_options, postings_firsts, tables })
}

/// A run of documents, each with its metadata and its text: what a result needs to be shown.
pub(super) fn encode_page(start: u32, documents: &[(&DocumentMeta, &str)]) -> Vec<u8> {
    let mut writer = Writer::new(PAGE_MAGIC);
    writer.u32(start);
    writer.u32(documents.len() as u32);
    for (document, content) in documents {
        encode_document(&mut writer, document);
        writer.string(content);
    }
    writer.finish()
}

pub(crate) fn decode_page(bytes: &[u8]) -> Result<(u32, Vec<(DocumentMeta, String)>), String> {
    let mut reader = Reader::new(bytes, PAGE_MAGIC)?;
    let start = reader.u32()?;
    let count = reader.u32()? as usize;
    let mut documents = Vec::with_capacity(count);
    for _ in 0..count {
        let document = decode_document(&mut reader)?;
        documents.push((document, reader.string()?));
    }
    reader.finish()?;
    Ok((start, documents))
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

#[cfg(test)]
mod tests {
    use super::{decode_postings, encode_postings};
    use crate::document::Posting;

    #[test]
    fn decoder_rejects_trailing_bytes() {
        let mut bytes = encode_postings(&[("示例".to_string(), vec![Posting { document: 0, title_hits: 1, body_hits: 0 }])]);
        bytes.push(0);
        assert_eq!(decode_postings(&bytes).unwrap_err(), "trailing bytes in search artifact");
    }
}
