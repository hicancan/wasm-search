import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ArtifactRef, BlockRef, SearchBundleManifest } from './artifacts.js';

const engine = vi.hoisted(() => ({ free: vi.fn(), loadedPages: vi.fn(), created: vi.fn() }));
const block = (pack: number): BlockRef => ({ id: 0, pack, offset: 0, bytes: 4, decoded_bytes: 4 });

vi.mock('../wasm/wasm_search_wasm.js', () => ({
    default: vi.fn().mockResolvedValue(undefined),
    SearchEngine: class {
        constructor() { engine.created(); }
        document_count() { return 2; }
        filter_options() { return JSON.stringify({ sources: [], categories: [], categoriesBySource: {} }); }
        begin_search() { return JSON.stringify([block(1)]); }
        plan_search() { return JSON.stringify([block(2)]); }
        load_postings_chunk() {}
        load_page(...args: unknown[]) { engine.loadedPages(...args); }
        hydrate_search() { return JSON.stringify({ totalCandidates: 2, results: [{ id: 'first', snippet: '正文摘要' }] }); }
        free() { engine.free(); }
    },
}));

import { SearchRuntime } from './runtime.js';

const manifest: SearchBundleManifest = {
    format: 'wasm-search-bundle-v6',
    index: { pack: 0, offset: 0, bytes: 4, decoded_bytes: 4 },
    packs: ['pack-000.bin', 'pack-001.bin', 'pack-002.bin'],
    profile: { aliases: [] },
};
const bytes = () => new Uint8Array([1, 2, 3, 4]).buffer;
const query = { query: '计算机等级', limit: 10, sort: 'relevance' as const, filters: {} };

function source(overrides: Partial<{ manifest: () => Promise<SearchBundleManifest> }> = {}) {
    return {
        manifest: overrides.manifest ?? vi.fn().mockResolvedValue(manifest),
        bytes: vi.fn().mockImplementation(() => Promise.resolve(bytes())),
        blocks: vi.fn().mockImplementation((references: ArtifactRef[]) => Promise.resolve(references.map(() => bytes()))),
    };
}

beforeEach(() => {
    vi.clearAllMocks();
});

describe('SearchRuntime', () => {
    it('reads postings, then pages, and returns hydrated results', async () => {
        const fake = source();
        const runtime = new SearchRuntime(fake as never, 1024);
        await runtime.initialize();
        await expect(runtime.search(query)).resolves.toEqual({ totalCandidates: 2, results: [{ id: 'first', snippet: '正文摘要' }] });
        expect(fake.blocks.mock.calls.map(([references]) => references.map((reference: BlockRef) => reference.pack))).toEqual([[0], [1], [2]]);
        expect(engine.loadedPages).toHaveBeenCalledTimes(1);
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
        await runtime.search(query);
        expect(engine.created).toHaveBeenCalledTimes(1);
        await runtime.search(query);
        expect(engine.created).toHaveBeenCalledTimes(2);
    });
});
