import initWasm, { SearchEngine as WasmSearchEngine } from '../wasm/wasm_search_wasm.js';
import { ArtifactSource, type BlockRef, type SearchBundleManifest } from './artifacts.js';
import type { FilterOptions, Query, SearchResponse } from './model.js';

/** Drives one engine through a query, fetching only the blocks it names. */
export class SearchRuntime {
    private readonly source: ArtifactSource;
    private readonly workingSetBudgetBytes: number;
    private manifestValue: SearchBundleManifest | null = null;
    private index: ArrayBuffer | null = null;
    private engine: WasmSearchEngine | null = null;
    private loadedBytes = 0;

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
        const [, manifest] = await Promise.all([initWasm(), this.source.manifest(signal)]);
        if (manifest.index.decoded_bytes > this.workingSetBudgetBytes) {
            throw new Error('search index exceeds the search memory budget');
        }
        const index = await this.source.bytes(manifest.index, signal);
        signal?.throwIfAborted();
        this.manifestValue = manifest;
        this.index = index;
        this.resetEngine();
        return {
            manifest,
            documentCount: this.requireEngine().document_count(),
            filterOptions: JSON.parse(this.requireEngine().filter_options()) as FilterOptions,
        };
    }

    private requireEngine(): WasmSearchEngine {
        if (!this.engine) throw new Error('search runtime is not initialized');
        return this.engine;
    }

    private resetEngine(): void {
        if (!this.index || !this.manifestValue) throw new Error('search index is not loaded');
        this.engine?.free();
        this.engine = new WasmSearchEngine(
            new Uint8Array(this.index),
            this.manifestValue.index.decoded_bytes,
            JSON.stringify(this.manifestValue.profile),
        );
        this.loadedBytes = this.manifestValue.index.decoded_bytes;
    }

    private async load(json: string, consume: (id: number, bytes: Uint8Array, decoded: number) => void, signal?: AbortSignal): Promise<void> {
        const blocks = JSON.parse(json) as BlockRef[];
        const loaded = await Promise.all(blocks.map(block => this.source.bytes(block, signal)));
        signal?.throwIfAborted();
        blocks.forEach((block, index) => {
            consume(block.id, new Uint8Array(loaded[index]!), block.decoded_bytes);
            this.loadedBytes += block.decoded_bytes;
        });
    }

    async search(
        query: Query,
        onRanked: (response: SearchResponse) => void,
        signal?: AbortSignal,
    ): Promise<SearchResponse> {
        if (query.query.trim().length === 1) {
            throw new Error('a query must be empty or contain at least two characters');
        }
        // Blocks only accumulate between queries; past the budget, start clean.
        if (this.loadedBytes > this.workingSetBudgetBytes) this.resetEngine();
        const engine = this.requireEngine();
        const request = JSON.stringify(query);
        await this.load(engine.begin_search(request), (id, bytes, decoded) => engine.load_lexicon_block(id, bytes, decoded), signal);
        await this.load(engine.required_posting_chunks(), (id, bytes, decoded) => engine.load_postings_chunk(id, bytes, decoded), signal);
        await this.load(engine.plan_search(), (id, bytes, decoded) => engine.load_document_block(id, bytes, decoded), signal);
        signal?.throwIfAborted();
        onRanked(JSON.parse(engine.ranked_results()) as SearchResponse);
        const before = this.loadedBytes;
        engine.clear_content();
        await this.load(engine.required_content_chunks(), (id, bytes, decoded) => engine.load_content_chunk(id, bytes, decoded), signal);
        signal?.throwIfAborted();
        this.loadedBytes = before;
        try {
            return JSON.parse(engine.hydrate_search()) as SearchResponse;
        } finally {
            engine.clear_content();
        }
    }

    dispose(): void {
        this.engine?.free();
        this.engine = null;
        this.manifestValue = null;
        this.index = null;
        this.loadedBytes = 0;
    }
}
