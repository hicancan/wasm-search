import { afterEach, describe, expect, it, vi } from 'vitest';

import { ArtifactSource } from './artifacts.js';
import { CacheStore } from './cache.js';
import type { ArtifactRef, SearchBundleManifest } from './artifacts.js';

const bytes = new Uint8Array([1, 2, 3]);
const reference: ArtifactRef = { path: 'documents-index.bin', bytes: bytes.byteLength, decoded_bytes: 3 };
const manifest: SearchBundleManifest = {
    format: 'wasm-search-bundle-v2',
    documents: { index: reference, blocks: [{ ...reference, path: 'documents-0000.bin' }] },
    lexicon: { index: { ...reference, path: 'lexicon-index.bin' }, blocks: [{ ...reference, path: 'lexicon-0000.bin' }] },
    postings: [{ ...reference, path: 'postings-0000.bin' }],
    content: [{ ...reference, path: 'content-0000.bin' }],
    profile: { aliases: [] },
};
const json = (value: unknown) => new Response(JSON.stringify(value), { status: 200 });

afterEach(() => {
    vi.unstubAllGlobals();
});

describe('ArtifactSource', () => {
    it('fetches files next to the manifest and caches them', async () => {
        const fetchMock = vi.fn().mockResolvedValueOnce(json(manifest)).mockResolvedValueOnce(new Response(bytes));
        vi.stubGlobal('fetch', fetchMock);
        const source = new ArtifactSource('/data/search/', new CacheStore(1024));

        await source.manifest();
        await source.bytes(reference);
        await source.bytes(reference);

        expect(fetchMock).toHaveBeenNthCalledWith(1, '/data/search/manifest.json', expect.anything());
        expect(fetchMock).toHaveBeenNthCalledWith(2, '/data/search/documents-index.bin', expect.anything());
        expect(fetchMock).toHaveBeenCalledTimes(2);
    });

    it('rejects another format and truncated files', async () => {
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(json({ ...manifest, format: 'other' })));
        await expect(new ArtifactSource('/s', new CacheStore(1024)).manifest()).rejects.toThrow('format');

        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(new Uint8Array([1]))));
        await expect(new ArtifactSource('/s', new CacheStore(1024)).bytes(reference)).rejects.toThrow('truncated');
    });
    it('bounds concurrent streams through body consumption when a query needs many blocks', async () => {
        let active = 0, peak = 0;
        vi.stubGlobal('fetch', vi.fn(async () => {
            active++;
            peak = Math.max(peak, active);
            return new Response(new ReadableStream({ start(controller) {
                setTimeout(() => { controller.enqueue(bytes); controller.close(); active--; }, 5);
            } }));
        }));
        const source = new ArtifactSource('/s', new CacheStore(1024));
        const result = await Promise.all(Array.from({ length: 40 }, (_, index) => source.bytes({ ...reference, path: `block-${index}.bin` })));
        expect(result).toHaveLength(40);
        expect(peak).toBe(16);
        expect(active).toBe(0);
    });
    it('does not send queued requests after their query is canceled', async () => {
        let finish!: () => void;
        const pending = new Promise<void>(resolve => { finish = resolve; });
        const fetchMock = vi.fn(async () => { await pending; return new Response(bytes); });
        vi.stubGlobal('fetch', fetchMock);
        const source = new ArtifactSource('/s', new CacheStore(1024));
        const controller = new AbortController();
        const requests = Array.from({ length: 40 }, (_, index) => source.bytes({ ...reference, path: `block-${index}.bin` }, controller.signal));
        const settled = Promise.allSettled(requests);
        controller.abort();
        finish();
        await settled;
        expect(fetchMock).toHaveBeenCalledTimes(16);
        expect(await source.bytes({ ...reference, path: 'next.bin' })).toEqual(bytes.buffer);
    });
    it('retries transient transport failure but never retries invalid HTTP or truncated data', async () => {
        const fetchMock = vi.fn().mockRejectedValueOnce(new TypeError('Failed to fetch')).mockResolvedValueOnce(new Response(bytes));
        vi.stubGlobal('fetch', fetchMock);
        expect(await new ArtifactSource('/s', new CacheStore(1024)).bytes(reference)).toEqual(bytes.buffer);
        expect(fetchMock).toHaveBeenCalledTimes(2);
        fetchMock.mockReset().mockResolvedValue(new Response('', { status: 404 }));
        await expect(new ArtifactSource('/s', new CacheStore(1024)).bytes(reference)).rejects.toThrow('404');
        expect(fetchMock).toHaveBeenCalledTimes(1);
    });
});
