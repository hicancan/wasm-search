use serde::{Deserialize, Serialize};

/// A file attached to a document.
#[derive(Clone, Debug, Deserialize)]
pub struct Attachment {
    pub url: String,
    pub name: String,
    #[serde(default, rename = "ext")]
    pub extension: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    #[default]
    Page,
    File,
    Link,
}

/// One searchable document, as read from a line of `documents.jsonl`.
#[derive(Clone, Debug, Deserialize)]
pub struct IndexDocument {
    pub id: String,
    pub source: String,
    #[serde(default)]
    pub source_name: String,
    pub url: String,
    pub title: String,
    #[serde(default, rename = "text")]
    pub content: String,
    #[serde(default, rename = "published")]
    pub published_at: Option<String>,
    #[serde(default, rename = "updated")]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub kind: DocumentKind,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, rename = "files")]
    pub attachments: Vec<Attachment>,
}

#[derive(Clone, Debug)]
pub(crate) struct DocumentMeta {
    pub id: String,
    pub source: String,
    pub source_name: String,
    pub url: String,
    pub title: String,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    pub section: Option<String>,
    pub kind: DocumentKind,
    pub category: Option<String>,
    pub attachments: Vec<Attachment>,
    pub content_chunk: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Posting {
    pub document: u32,
    pub title_hits: u16,
    pub body_hits: u16,
}
