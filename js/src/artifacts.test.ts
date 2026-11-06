import { afterEach, describe, expect, it, vi } from 'vitest';

import { ArtifactSource } from './artifacts.js';
import { CacheStore } from './cache.js';
import type { ArtifactRef, SearchBundleManifest } from './artifacts.js';

const bytes = new Uint8Array([1, 2, 3]);
const reference: ArtifactRef = { pack: 0, offset: 0, bytes: bytes.byteLength, decoded_bytes: 3 };
const manifest: SearchBundleManifest = { format: 'wasm-search-bundle-v3', index: reference, packs: ['pack-000.bin'], profile: { aliases: [] } };
const json = (value: unknown) => new Response(JSON.stringify(value), { status: 200 });
const ranged = (body: BodyInit = bytes) => new Response(body, { status: 206 });
/** A server: the manifest by name, everything else from `pack`. */
const serve = (pack: () => Response | Promise<Response>) => vi.fn(async (url: string) => url.endsWith('manifest.json') ? json(manifest) : pack());
const packCalls = (mock: ReturnType<typeof serve>) => mock.mock.calls.filter(([url]) => !url.endsWith('manifest.json'));

afterEach(() => {
    vi.unstubAllGlobals();
});

describe('ArtifactSource', () => {
    it('reads a block by byte range from its pack and caches it', async () => {
        const fetchMock = serve(() => ranged());
        vi.stubGlobal('fetch', fetchMock);
        const source = new ArtifactSource('/data/search/', new CacheStore(1024));

        await source.bytes(reference);
        await source.bytes(reference);

        const calls = packCalls(fetchMock);
        expect(calls).toHaveLength(1);
        expect(calls[0]).toEqual(['/data/search/pack-000.bin', expect.objectContaining({ headers: { Range: 'bytes=0-2' } })]);
    });

    it('takes the block out of a whole pack when a server ignores the range', async () => {
        vi.stubGlobal('fetch', serve(() => new Response(new Uint8Array([9, 9, 1, 2, 3, 9]), { status: 200 })));
        const source = new ArtifactSource('/s', new CacheStore(1024));
        expect(new Uint8Array(await source.bytes({ ...reference, offset: 2 }))).toEqual(bytes);
    });

    it('prefers a mirror and falls back to the pack when it fails', async () => {
        const fetchMock = vi.fn(async (url: string) => {
            if (url.endsWith('manifest.json')) return json({ ...manifest, mirrors: ['https://mirror.test/pack?sig=1'] });
            return url.startsWith('https://mirror.test') ? new Response('', { status: 403 }) : ranged();
        });
        vi.stubGlobal('fetch', fetchMock);
        expect(await new ArtifactSource('/s', new CacheStore(1024)).bytes(reference)).toEqual(bytes.buffer);
        expect(fetchMock.mock.calls.map(([url]) => url)).toEqual(['/s/manifest.json', 'https://mirror.test/pack?sig=1', '/s/pack-000.bin']);
    });

    it('rejects another format and truncated blocks', async () => {
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(json({ ...manifest, format: 'other' })));
        await expect(new ArtifactSource('/s', new CacheStore(1024)).manifest()).rejects.toThrow('format');

        vi.stubGlobal('fetch', serve(() => ranged(new Uint8Array([1]))));
        await expect(new ArtifactSource('/s', new CacheStore(1024)).bytes(reference)).rejects.toThrow('truncat');
    });

    it('bounds concurrent streams through body consumption when a query needs many blocks', async () => {
        let active = 0, peak = 0;
        vi.stubGlobal('fetch', serve(async () => {
            active++;
            peak = Math.max(peak, active);
            return ranged(new ReadableStream({ start(controller) {
                setTimeout(() => { controller.enqueue(bytes); controller.close(); active--; }, 5);
            } }));
        }));
        const source = new ArtifactSource('/s', new CacheStore(1024));
        const result = await Promise.all(Array.from({ length: 40 }, (_, index) => source.bytes({ ...reference, offset: index * 3 })));
        expect(result).toHaveLength(40);
        expect(peak).toBe(16);
        expect(active).toBe(0);
    });

    it('retries transient transport failure but never retries invalid HTTP', async () => {
        let failures = 1;
        const flaky = serve(() => { if (failures-- > 0) throw new TypeError('Failed to fetch'); return ranged(); });
        vi.stubGlobal('fetch', flaky);
        expect(await new ArtifactSource('/s', new CacheStore(1024)).bytes(reference)).toEqual(bytes.buffer);
        expect(packCalls(flaky)).toHaveLength(2);

        const missing = serve(() => new Response('', { status: 404 }));
        vi.stubGlobal('fetch', missing);
        await expect(new ArtifactSource('/s', new CacheStore(1024)).bytes(reference)).rejects.toThrow('404');
        expect(packCalls(missing)).toHaveLength(1);
    });
});
