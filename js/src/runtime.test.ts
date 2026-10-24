import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ArtifactRef, SearchBundleManifest } from './artifacts';

const engine = vi.hoisted(() => ({
    clearContent: vi.fn(),
    free: vi.fn(),
    loadedContent: vi.fn(),
    loadedPostings: vi.fn(),
}));

vi.mock('../wasm/wasm_search_wasm.js', () => ({
    default: vi.fn().mockResolvedValue(undefined),
    SearchEngine: class {
        document_count() { return 2; }
        filter_options() { return JSON.stringify({ sources: [], categories: [], categoriesBySource: {} }); }
        begin_search() { return JSON.stringify([0]); }
        load_postings_chunk(...args: unknown[]) { engine.loadedPostings(...args); }
        prepare_search() {
            return JSON.stringify({
                totalCandidates: 2,
                results: [{ id: 'first', snippet: null }],
            });
        }
        required_content_chunks() { return JSON.stringify([0]); }
        load_content_chunk(...args: unknown[]) { engine.loadedContent(...args); }
        hydrate_search() {
            return JSON.stringify({
                totalCandidates: 2,
                results: [{ id: 'first', snippet: '正文摘要' }],
            });
        }
        clear_content() { engine.clearContent(); }
        free() { engine.free(); }
    },
}));

import { SearchRuntime } from './runtime';

const artifact = (path: string, decodedBytes = 4): ArtifactRef => ({
    path,
    bytes: 4,
    decoded_bytes: decodedBytes,
});

const manifest: SearchBundleManifest = {
    format: 'wasm-search-bundle-v1',
    documents: artifact('documents.bin'),
    lexicon: artifact('lexicon.bin'),
    postings: [artifact('postings-0000.bin')],
    content: [artifact('content-0000.bin')],
    profile: { aliases: [] },
};

function source(overrides: Partial<{
    manifest: () => Promise<SearchBundleManifest>;
    bytes: (reference: ArtifactRef) => Promise<ArrayBuffer>;
}> = {}) {
    return {
        manifest: overrides.manifest ?? vi.fn().mockResolvedValue(manifest),
        bytes: overrides.bytes ?? vi.fn().mockResolvedValue(new Uint8Array([1, 2, 3, 4]).buffer),
    };
}

beforeEach(() => {
    vi.clearAllMocks();
});

describe('SearchRuntime', () => {
    it('publishes exact shells before content and releases hydrated content', async () => {
        let releaseContent: ((value: ArrayBuffer) => void) | undefined;
        const content = new Promise<ArrayBuffer>(resolve => { releaseContent = resolve; });
        const fake = source({
            bytes: vi.fn((reference: ArtifactRef) => (
                reference.path.startsWith('content-')
                    ? content
                    : Promise.resolve(new Uint8Array([1, 2, 3, 4]).buffer)
            )),
        });
        const runtime = new SearchRuntime(fake as never, 1024);
        await runtime.initialize();
        const onRanked = vi.fn();
        const result = runtime.search({
            query: '计算机等级',
            limit: 10,
            sort: 'relevance',
            filters: {},
        }, onRanked);

        await vi.waitFor(() => expect(onRanked).toHaveBeenCalledWith({
            totalCandidates: 2,
            results: [{ id: 'first', snippet: null }],
        }));
        expect(engine.loadedContent).not.toHaveBeenCalled();
        releaseContent?.(new Uint8Array([1, 2, 3, 4]).buffer);

        await expect(result).resolves.toEqual({
            totalCandidates: 2,
            results: [{ id: 'first', snippet: '正文摘要' }],
        });
        expect(engine.clearContent).toHaveBeenCalledTimes(2);
        runtime.dispose();
    });

    it('rejects metadata that cannot fit in the decoded working-set budget', async () => {
        const oversized = {
            ...manifest,
            documents: artifact('documents.bin', 800),
            lexicon: artifact('lexicon.bin', 400),
        };
        const runtime = new SearchRuntime(source({
            manifest: vi.fn().mockResolvedValue(oversized),
        }) as never, 1024);

        await expect(runtime.initialize()).rejects.toThrow(
            'search metadata exceeds the search memory budget',
        );
    });

    it('rejects a query whose postings cannot fit after resetting the engine', async () => {
        const largePosting = {
            ...manifest,
            postings: [artifact('postings-0000.bin', 32)],
        };
        const fake = source({ manifest: vi.fn().mockResolvedValue(largePosting) });
        const runtime = new SearchRuntime(fake as never, 24);
        await runtime.initialize();

        await expect(runtime.search({
            query: '奖学金',
            limit: 10,
            sort: 'relevance',
            filters: {},
        }, vi.fn())).rejects.toThrow('query postings exceed the search memory budget');
    });
});
