use crate::document::{Attachment, DocumentKind, DocumentMeta, Posting};

const DOCUMENT_MAGIC: &[u8; 8] = b"WSDOCS01";
const LEXICON_MAGIC: &[u8; 8] = b"WSLEXI01";
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

pub(super) fn encode_documents(documents: &[DocumentMeta]) -> Vec<u8> {
    let mut writer = Writer::new(DOCUMENT_MAGIC);
    writer.u32(documents.len() as u32);
    for document in documents {
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
    writer.finish()
}

pub(crate) fn decode_documents(bytes: &[u8]) -> Result<Vec<DocumentMeta>, String> {
    let mut reader = Reader::new(bytes, DOCUMENT_MAGIC)?;
    let count = reader.u32()? as usize;
    let mut documents = Vec::with_capacity(count);
    for _ in 0..count {
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
        documents.push(DocumentMeta {
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
        });
    }
    reader.finish()?;
    Ok(documents)
}

pub(super) fn encode_lexicon(entries: &[(String, u32)]) -> Vec<u8> {
    let mut writer = Writer::new(LEXICON_MAGIC);
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

pub(crate) fn decode_lexicon(bytes: &[u8]) -> Result<Vec<(String, u32)>, String> {
    let mut reader = Reader::new(bytes, LEXICON_MAGIC)?;
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
    use super::{decode_lexicon, encode_lexicon};

    #[test]
    fn decoder_rejects_trailing_bytes() {
        let mut bytes = encode_lexicon(&[("南邮".to_string(), 0)]);
        bytes.push(0);
        assert_eq!(
            decode_lexicon(&bytes).unwrap_err(),
            "trailing bytes in search artifact"
        );
    }
}
