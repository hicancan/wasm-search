import initWasm, { SearchEngine as WasmSearchEngine } from '../wasm/wasm_search_wasm.js';
import { ArtifactSource, type ArtifactRef, type SearchBundleManifest } from './artifacts.js';
import type { FilterOptions, Query, SearchResponse } from './model.js';

export class SearchRuntime {
    private readonly source: ArtifactSource;
    private readonly workingSetBudgetBytes: number;
    private manifestValue: SearchBundleManifest | null = null;
    private documents: ArrayBuffer | null = null;
    private lexiconIndex: ArrayBuffer | null = null;
    private engine: WasmSearchEngine | null = null;
    private readonly loadedPostings = new Map<string, number>();
    private readonly loadedLexiconBlocks = new Map<string, number>();
    private readonly loadedDocumentBlocks = new Map<string, number>();
    private postingsBytes = 0;
    private lexiconBytes = 0;
    private documentBytes = 0;
    private metadataBytes = 0;

    constructor(source: ArtifactSource, workingSetBudgetBytes: number) {
        if (!Number.isSafeInteger(workingSetBudgetBytes) || workingSetBudgetBytes <= 0) {
            throw new Error('working-set budget must be a positive integer');
        }
        this.source = source;
        this.workingSetBudgetBytes = workingSetBudgetBytes;
    }

    async initialize(signal?: AbortSignal): Promise<{
        manifest: SearchBundleManifest;
        documentCount: number;
        filterOptions: FilterOptions;
    }> {
        const [, manifest] = await Promise.all([
            initWasm(),
            this.source.manifest(signal),
        ]);
        this.metadataBytes = manifest.documents.index.decoded_bytes + manifest.lexicon.index.decoded_bytes;
        if (this.metadataBytes > this.workingSetBudgetBytes) {
            throw new Error('search metadata exceeds the search memory budget');
        }
        const [documents, lexiconIndex] = await Promise.all([
            this.source.bytes(manifest.documents.index, signal),
            this.source.bytes(manifest.lexicon.index, signal),
        ]);
        signal?.throwIfAborted();
        this.manifestValue = manifest;
        this.documents = documents;
        this.lexiconIndex = lexiconIndex;
        this.resetEngine();
        return {
            manifest,
            documentCount: this.requireEngine().document_count(),
            filterOptions: JSON.parse(this.requireEngine().filter_options()) as FilterOptions,
        };
    }

    private requireManifest(): SearchBundleManifest {
        if (!this.manifestValue) throw new Error('search runtime is not initialized');
        return this.manifestValue;
    }

    private requireEngine(): WasmSearchEngine {
        if (!this.engine) throw new Error('search runtime is not initialized');
        return this.engine;
    }

    private resetEngine(): void {
        if (!this.documents || !this.lexiconIndex) throw new Error('search metadata is not loaded');
        this.engine?.free();
        this.engine = new WasmSearchEngine(
            new Uint8Array(this.documents),
            this.requireManifest().documents.index.decoded_bytes,
            new Uint8Array(this.lexiconIndex),
            this.requireManifest().lexicon.index.decoded_bytes,
            JSON.stringify(this.requireManifest().profile),
        );
        this.loadedPostings.clear();
        this.loadedLexiconBlocks.clear();
        this.loadedDocumentBlocks.clear();
        this.postingsBytes = 0;
        this.lexiconBytes = 0;
        this.documentBytes = 0;
    }

    private references(kind: 'postings' | 'content', chunks: number[]): ArtifactRef[] {
        const references = this.requireManifest()[kind];
        return chunks.map(chunk => {
            const reference = references[chunk];
            if (!reference) throw new Error(`search bundle ${kind} chunk does not exist: ${chunk}`);
            return reference;
        });
    }

    private missingPostingBytes(chunks: number[]): number {
        return this.references('postings', chunks)
            .filter(reference => !this.loadedPostings.has(reference.path))
            .reduce((total, reference) => total + reference.decoded_bytes, 0);
    }

    private missingLexiconBytes(blocks: number[]): number {
        return blocks
            .map(block => this.requireManifest().lexicon.blocks[block])
            .filter((reference): reference is ArtifactRef => Boolean(reference))
            .filter(reference => !this.loadedLexiconBlocks.has(reference.path))
            .reduce((total, reference) => total + reference.decoded_bytes, 0);
    }

    private missingDocumentBytes(blocks: number[]): number {
        return blocks
            .map(block => this.requireManifest().documents.blocks[block])
            .filter((reference): reference is ArtifactRef => Boolean(reference))
            .filter(reference => !this.loadedDocumentBlocks.has(reference.path))
            .reduce((total, reference) => total + reference.decoded_bytes, 0);
    }

    private async loadLexiconBlocks(blocks: number[], signal?: AbortSignal): Promise<void> {
        const missing = blocks
            .map((block) => ({ block, reference: this.requireManifest().lexicon.blocks[block] }))
            .filter(item => item.reference && !this.loadedLexiconBlocks.has(item.reference.path));
        const loaded = await Promise.all(missing.map(async item => ({
            ...item,
            bytes: await this.source.bytes(item.reference, signal),
        })));
        signal?.throwIfAborted();
        const engine = this.requireEngine();
        for (const item of loaded) {
            engine.load_lexicon_block(
                item.block,
                new Uint8Array(item.bytes),
                item.reference.decoded_bytes,
            );
            this.loadedLexiconBlocks.set(item.reference.path, item.reference.decoded_bytes);
            this.lexiconBytes += item.reference.decoded_bytes;
        }
    }

    private async loadDocumentBlocks(blocks: number[], signal?: AbortSignal): Promise<void> {
        const missing = blocks
            .map((block) => ({ block, reference: this.requireManifest().documents.blocks[block] }))
            .filter(item => item.reference && !this.loadedDocumentBlocks.has(item.reference.path));
        const loaded = await Promise.all(missing.map(async item => ({
            ...item,
            bytes: await this.source.bytes(item.reference, signal),
        })));
        signal?.throwIfAborted();
        const engine = this.requireEngine();
        for (const item of loaded) {
            engine.load_document_block(
                item.block,
                new Uint8Array(item.bytes),
                item.reference.decoded_bytes,
            );
            this.loadedDocumentBlocks.set(item.reference.path, item.reference.decoded_bytes);
            this.documentBytes += item.reference.decoded_bytes;
        }
    }

    private async loadPostings(chunks: number[], signal?: AbortSignal): Promise<void> {
        const missing = this.references('postings', chunks)
            .map((reference, index) => ({ reference, chunk: chunks[index] }))
            .filter(item => item.chunk !== undefined && !this.loadedPostings.has(item.reference.path));
        const loaded = await Promise.all(missing.map(async item => ({
            ...item,
            bytes: await this.source.bytes(item.reference, signal),
        })));
        signal?.throwIfAborted();
        const engine = this.requireEngine();
        for (const item of loaded) {
            engine.load_postings_chunk(
                item.chunk as number,
                new Uint8Array(item.bytes),
                item.reference.decoded_bytes,
            );
            this.loadedPostings.set(item.reference.path, item.reference.decoded_bytes);
            this.postingsBytes += item.reference.decoded_bytes;
        }
    }

    private async ensurePostings(query: Query, signal?: AbortSignal): Promise<void> {
        const request = JSON.stringify(query);
        let lexiconBlocks = JSON.parse(this.requireEngine().begin_search(request)) as number[];
        let missingBytes = this.missingLexiconBytes(lexiconBlocks);
        if (this.metadataBytes + this.lexiconBytes + this.postingsBytes + missingBytes > this.workingSetBudgetBytes) {
            this.resetEngine();
            lexiconBlocks = JSON.parse(this.requireEngine().begin_search(request)) as number[];
            missingBytes = this.missingLexiconBytes(lexiconBlocks);
        }
        if (this.metadataBytes + missingBytes > this.workingSetBudgetBytes) {
            throw new Error('query lexicon exceeds the search memory budget');
        }
        await this.loadLexiconBlocks(lexiconBlocks, signal);
        const chunks = JSON.parse(this.requireEngine().required_posting_chunks()) as number[];
        const missingPostingBytes = this.missingPostingBytes(chunks);
        if (this.metadataBytes + this.lexiconBytes + this.postingsBytes + missingPostingBytes > this.workingSetBudgetBytes) {
            throw new Error('query postings exceed the search memory budget');
        }
        await this.loadPostings(chunks, signal);
        const documentBlocks = JSON.parse(this.requireEngine().required_document_blocks()) as number[];
        const missingDocumentBytes = this.missingDocumentBytes(documentBlocks);
        if (this.metadataBytes + this.lexiconBytes + this.documentBytes + this.postingsBytes + missingDocumentBytes > this.workingSetBudgetBytes) {
            throw new Error('query document blocks exceed the search memory budget');
        }
        await this.loadDocumentBlocks(documentBlocks, signal);
    }

    private async loadContent(chunks: number[], signal?: AbortSignal): Promise<void> {
        const references = this.references('content', chunks);
        const decodedBytes = references.reduce(
            (total, reference) => total + reference.decoded_bytes,
            0,
        );
        if (this.metadataBytes + this.lexiconBytes + this.documentBytes + this.postingsBytes + decodedBytes > this.workingSetBudgetBytes) {
            throw new Error('query content exceeds the search memory budget');
        }
        const loaded = await Promise.all(references.map(async (reference, index) => ({
            reference,
            chunk: chunks[index],
            bytes: await this.source.bytes(reference, signal),
        })));
        signal?.throwIfAborted();
        const engine = this.requireEngine();
        engine.clear_content();
        for (const item of loaded) {
            if (item.chunk === undefined) throw new Error('missing content chunk index');
            engine.load_content_chunk(
                item.chunk,
                new Uint8Array(item.bytes),
                item.reference.decoded_bytes,
            );
        }
    }

    async search(
        query: Query,
        onRanked: (response: SearchResponse) => void,
        signal?: AbortSignal,
    ): Promise<SearchResponse> {
        if (query.query.trim().length < 2) {
            throw new Error('query must contain at least two characters');
        }
        await this.ensurePostings(query, signal);
        signal?.throwIfAborted();
        const engine = this.requireEngine();
        const ranked = JSON.parse(engine.prepare_search()) as SearchResponse;
        signal?.throwIfAborted();
        onRanked(ranked);

        const contentChunks = JSON.parse(
            engine.required_content_chunks(0, query.limit),
        ) as number[];
        await this.loadContent(contentChunks, signal);
        signal?.throwIfAborted();
        try {
            return JSON.parse(engine.hydrate_search(0, query.limit)) as SearchResponse;
        } finally {
            engine.clear_content();
        }
    }

    dispose(): void {
        this.engine?.free();
        this.engine = null;
        this.manifestValue = null;
        this.documents = null;
        this.lexiconIndex = null;
        this.loadedPostings.clear();
        this.loadedLexiconBlocks.clear();
        this.loadedDocumentBlocks.clear();
        this.postingsBytes = 0;
        this.lexiconBytes = 0;
        this.documentBytes = 0;
        this.metadataBytes = 0;
    }
}
