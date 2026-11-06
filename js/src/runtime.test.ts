import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ArtifactRef, BlockRef, SearchBundleManifest } from './artifacts.js';

const engine = vi.hoisted(() => ({ clearContent: vi.fn(), free: vi.fn(), loadedContent: vi.fn(), created: vi.fn() }));
const block = (pack: number): BlockRef => ({ id: 0, pack, offset: 0, bytes: 4, decoded_bytes: 4 });

vi.mock('../wasm/wasm_search_wasm.js', () => ({
    default: vi.fn().mockResolvedValue(undefined),
    SearchEngine: class {
        constructor() { engine.created(); }
        document_count() { return 2; }
        filter_options() { return JSON.stringify({ sources: [], categories: [], categoriesBySource: {} }); }
        begin_search() { return JSON.stringify([block(1)]); }
        required_posting_chunks() { return JSON.stringify([block(2)]); }
        plan_search() { return JSON.stringify([block(3)]); }
        ranked_results() { return JSON.stringify({ totalCandidates: 2, results: [{ id: 'first', snippet: null }] }); }
        required_content_chunks() { return JSON.stringify([block(4)]); }
        load_lexicon_block() {}
        load_postings_chunk() {}
        load_document_block() {}
        load_content_chunk(...args: unknown[]) { engine.loadedContent(...args); }
        hydrate_search() { return JSON.stringify({ totalCandidates: 2, results: [{ id: 'first', snippet: '正文摘要' }] }); }
        clear_content() { engine.clearContent(); }
        free() { engine.free(); }
    },
}));

import { SearchRuntime } from './runtime.js';

const manifest: SearchBundleManifest = {
    format: 'wasm-search-bundle-v3',
    index: { pack: 0, offset: 0, bytes: 4, decoded_bytes: 4 },
    packs: ['pack-000.bin', 'pack-001.bin', 'pack-002.bin', 'pack-003.bin', 'pack-004.bin'],
    profile: { aliases: [] },
};
const bytes = () => new Uint8Array([1, 2, 3, 4]).buffer;
const query = { query: '计算机等级', limit: 10, sort: 'relevance' as const, filters: {} };

function source(overrides: Partial<{ manifest: () => Promise<SearchBundleManifest>; bytes: (reference: ArtifactRef) => Promise<ArrayBuffer> }> = {}) {
    return {
        manifest: overrides.manifest ?? vi.fn().mockResolvedValue(manifest),
        bytes: overrides.bytes ?? vi.fn().mockImplementation(() => Promise.resolve(bytes())),
    };
}

beforeEach(() => {
    vi.clearAllMocks();
});

describe('SearchRuntime', () => {
    it('publishes exact shells before content and releases hydrated content', async () => {
        let releaseContent: ((value: ArrayBuffer) => void) | undefined;
        const content = new Promise<ArrayBuffer>(resolve => { releaseContent = resolve; });
        const fake = source({ bytes: vi.fn((reference: ArtifactRef) => reference.pack === 4 ? content : Promise.resolve(bytes())) });
        const runtime = new SearchRuntime(fake as never, 1024);
        await runtime.initialize();
        const onRanked = vi.fn();
        const result = runtime.search(query, onRanked);

        await vi.waitFor(() => expect(onRanked).toHaveBeenCalledWith({ totalCandidates: 2, results: [{ id: 'first', snippet: null }] }));
        expect(engine.loadedContent).not.toHaveBeenCalled();
        releaseContent?.(bytes());

        await expect(result).resolves.toEqual({ totalCandidates: 2, results: [{ id: 'first', snippet: '正文摘要' }] });
        expect(engine.clearContent).toHaveBeenCalledTimes(2);
        runtime.dispose();
    });

    it('rejects an index that cannot fit in the decoded working-set budget', async () => {
        const runtime = new SearchRuntime(source({
            manifest: vi.fn().mockResolvedValue({ ...manifest, index: { ...manifest.index, decoded_bytes: 2048 } }),
        }) as never, 1024);
        await expect(runtime.initialize()).rejects.toThrow('search index exceeds the search memory budget');
    });

    it('starts a clean engine once loaded blocks pass the budget', async () => {
        const runtime = new SearchRuntime(source() as never, 8);
        await runtime.initialize();
        await runtime.search(query, vi.fn());
        expect(engine.created).toHaveBeenCalledTimes(1);
        await runtime.search(query, vi.fn());
        expect(engine.created).toHaveBeenCalledTimes(2);
    });
});
