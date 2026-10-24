export type SortMode = 'relevance' | 'date_desc';

export interface SearchFilters {
    sourceId?: string;
    excludedSourceIds?: string[];
    category?: string;
    publishedFrom?: string;
    publishedTo?: string;
    includeUndated?: boolean;
}

export interface Query {
    query: string;
    limit: number;
    sort: SortMode;
    filters: SearchFilters;
}

export interface SearchAttachment {
    url: string;
    name: string;
    extension: string | null;
}

export type DocumentKind = 'page' | 'file' | 'link';

export interface SearchResult {
    id: string;
    source: string;
    sourceName: string;
    url: string;
    title: string;
    publishedAt: string | null;
    updatedAt: string | null;
    section: string | null;
    kind: DocumentKind;
    category: string | null;
    snippet: string | null;
    matchedTerms: string[];
    attachments: SearchAttachment[];
}

export interface SearchResponse {
    totalCandidates: number;
    results: SearchResult[];
}

export interface FilterOption {
    id: string;
    label: string;
    count: number;
}

export interface FilterOptions {
    sources: FilterOption[];
    categories: FilterOption[];
    categoriesBySource: Record<string, FilterOption[]>;
}

/** A query equal to one of `terms` searches `phrases` and prefers `prefer_sources`. */
export interface Alias {
    terms: string[];
    phrases: string[];
    prefer_sources?: string[];
}

export interface Profile {
    aliases: Alias[];
}

/** One line of documents.jsonl. */
export interface SearchDocument {
    id: string;
    source: string;
    source_name?: string;
    url: string;
    title: string;
    text?: string;
    published?: string | null;
    updated?: string | null;
    section?: string | null;
    kind?: DocumentKind;
    category?: string | null;
    tags?: string[];
    files?: { url: string; name: string; ext?: string | null }[];
}
