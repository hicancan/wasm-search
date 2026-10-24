import { afterEach, describe, expect, it, vi } from 'vitest';

import { ArtifactSource } from './artifacts.js';
import { CacheStore } from './cache.js';
import type { ArtifactRef, SearchBundleManifest } from './artifacts.js';

const bytes = new Uint8Array([1, 2, 3]);
const reference: ArtifactRef = { path: 'documents.bin', bytes: bytes.byteLength, decoded_bytes: 3 };
const manifest: SearchBundleManifest = {
    format: 'wasm-search-bundle-v1',
    documents: reference,
    lexicon: { ...reference, path: 'lexicon.bin' },
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
        expect(fetchMock).toHaveBeenNthCalledWith(2, '/data/search/documents.bin', expect.anything());
        expect(fetchMock).toHaveBeenCalledTimes(2);
    });

    it('rejects another format and truncated files', async () => {
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(json({ ...manifest, format: 'other' })));
        await expect(new ArtifactSource('/s', new CacheStore(1024)).manifest()).rejects.toThrow('format');

        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(new Uint8Array([1]))));
        await expect(new ArtifactSource('/s', new CacheStore(1024)).bytes(reference)).rejects.toThrow('truncated');
    });
});
