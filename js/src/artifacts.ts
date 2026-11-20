import { CacheStore } from './cache.js';
import type { Profile } from './model.js';

export interface ArtifactRef {
    pack: number;
    offset: number;
    bytes: number;
    decoded_bytes: number;
}

/** A block the engine asks for, handed back under `id`. */
export interface BlockRef extends ArtifactRef {
    id: number;
}

export interface SearchBundleManifest {
    format: 'wasm-search-bundle-v5';
    index: ArtifactRef;
    packs: string[];
    /** Faster addresses for the packs, by position; `packs` remains the fallback. */
    mirrors?: string[];
    profile: Profile;
}

function parseManifest(value: unknown): SearchBundleManifest {
    const manifest = value as Partial<SearchBundleManifest> | null;
    if (!manifest || manifest.format !== 'wasm-search-bundle-v5') {
        throw new Error('unsupported search bundle format');
    }
    if (!manifest.index || !manifest.packs?.length) {
        throw new Error('incomplete search bundle manifest');
    }
    return manifest as SearchBundleManifest;
}

/** Blocks of one pack closer than this are read as one range: a little extra data costs less than a round trip. */
const COALESCE_GAP_BYTES = 16 * 1024;
const PERSISTENT_PREFIX = 'wasm-search ';

/**
 * Fetches a bundle's files from one directory URL. Blocks are kept in an LRU byte cache and, where the
 * browser offers Cache Storage, persistently: a bundle is immutable, so a block read once is never read
 * from the network again until the bundle changes, and then the old bundle's cache is dropped.
 */
export class ArtifactSource {
    private readonly baseUrl: string;
    private readonly cache: CacheStore;
    private manifestRequest: Promise<SearchBundleManifest> | null = null;
    private persistentCache: Promise<Cache | null> | null = null;
    private readonly requests = new Map<string, Promise<ArrayBuffer>>();
    private activeReads = 0;
    private readonly waitingReads: Array<() => void> = [];

    constructor(baseUrl: string, cache: CacheStore) {
        this.baseUrl = baseUrl.replace(/\/+$/, '');
        this.cache = cache;
    }

    manifest(signal?: AbortSignal): Promise<SearchBundleManifest> {
        this.manifestRequest ??= this.read(`${this.baseUrl}/manifest.json`, async response => parseManifest(await response.json()), signal).catch(error => {
            this.manifestRequest = null;
            throw error;
        });
        return this.manifestRequest;
    }

    /** One block by byte range; a server that ignores the range still yields the same bytes. */
    async bytes(reference: ArtifactRef, signal?: AbortSignal): Promise<ArrayBuffer> {
        return (await this.blocks([reference], signal))[0]!;
    }

    /**
     * Several blocks, in the order asked: from memory, then from persistent cache, then from the network,
     * where neighbouring blocks of one pack share a single range request.
     */
    async blocks(references: ArtifactRef[], signal?: AbortSignal): Promise<ArrayBuffer[]> {
        const manifest = await this.manifest(signal);
        const persistent = await this.persistent();
        const key = (reference: ArtifactRef) => `${reference.pack}:${reference.offset}`;
        const found = new Map<string, Promise<ArrayBuffer>>();
        const missing: ArtifactRef[] = [];
        for (const reference of references) {
            const id = key(reference);
            if (found.has(id)) continue;
            const cached = this.cache.get(id);
            if (cached) { found.set(id, Promise.resolve(cached)); continue; }
            const active = this.requests.get(id);
            if (active) { found.set(id, active); continue; }
            missing.push(reference);
        }
        const stored = await Promise.all(missing.map(reference => persistent?.match(this.cacheUrl(manifest, reference)).then(response => response?.arrayBuffer()) ?? Promise.resolve(undefined)));
        const network: ArtifactRef[] = [];
        missing.forEach((reference, index) => {
            const bytes = stored[index];
            if (bytes && bytes.byteLength === reference.bytes) {
                this.cache.set(key(reference), bytes);
                found.set(key(reference), Promise.resolve(bytes));
            } else network.push(reference);
        });
        for (const group of coalesce(network)) {
            const read = this.range(manifest, group, signal);
            group.forEach((reference, index) => {
                const id = key(reference);
                const one = read.then(parts => {
                    const bytes = parts[index]!;
                    this.cache.set(id, bytes);
                    void persistent?.put(this.cacheUrl(manifest, reference), new Response(bytes)).catch(() => undefined);
                    return bytes;
                }).finally(() => this.requests.delete(id));
                this.requests.set(id, one);
                found.set(id, one);
            });
        }
        return Promise.all(references.map(reference => found.get(key(reference))!));
    }

    /** One range covering a run of blocks of one pack, split back into the blocks. */
    private range(manifest: SearchBundleManifest, group: ArtifactRef[], signal?: AbortSignal): Promise<ArrayBuffer[]> {
        const first = group[0]!;
        const pack = first.pack;
        const path = manifest.packs[pack];
        if (!path) return Promise.reject(new Error(`search bundle pack does not exist: ${pack}`));
        const start = first.offset;
        const end = Math.max(...group.map(reference => reference.offset + reference.bytes));
        const range = `bytes=${start}-${end - 1}`;
        const decode = async (response: Response) => {
            const body = await response.arrayBuffer();
            const window = response.status === 206 ? body : body.slice(start, end);
            if (window.byteLength !== end - start) throw new Error(`truncated ${path}`);
            return group.map(reference => window.slice(reference.offset - start, reference.offset - start + reference.bytes));
        };
        const mirror = manifest.mirrors?.[pack];
        if (!mirror) return this.read(`${this.baseUrl}/${path}`, decode, signal, range);
        return this.read(mirror, decode, signal, range).catch(error => {
            if (signal?.aborted) throw error;
            return this.read(`${this.baseUrl}/${path}`, decode, signal, range);
        });
    }

    private cacheUrl(manifest: SearchBundleManifest, reference: ArtifactRef): string {
        return `${this.baseUrl}/${manifest.packs[reference.pack]}?block=${reference.offset}-${reference.bytes}`;
    }

    /** This bundle's persistent cache; caches of other bundles are dropped when it is first opened. */
    private persistent(): Promise<Cache | null> {
        this.persistentCache ??= (async () => {
            if (typeof caches === 'undefined') return null;
            try {
                const name = PERSISTENT_PREFIX + this.baseUrl;
                for (const other of await caches.keys()) {
                    if (other.startsWith(PERSISTENT_PREFIX) && other !== name) await caches.delete(other);
                }
                return await caches.open(name);
            } catch {
                return null;
            }
        })();
        return this.persistentCache;
    }

    /** Keep the slot until the body is consumed: HTTP/2 streams outlive response headers. */
    private async read<T>(url: string, decode: (response: Response) => Promise<T>, signal?: AbortSignal, range?: string): Promise<T> {
        signal?.throwIfAborted();
        if (this.activeReads >= 16) {
            await new Promise<void>((resolve, reject) => {
                const resume = () => {
                    signal?.removeEventListener('abort', abort);
                    this.activeReads++;
                    resolve();
                };
                const abort = () => {
                    const index = this.waitingReads.indexOf(resume);
                    if (index >= 0) this.waitingReads.splice(index, 1);
                    reject(signal?.reason);
                };
                this.waitingReads.push(resume);
                signal?.addEventListener('abort', abort, { once: true });
            });
        } else this.activeReads++;
        try {
            for (let attempt = 0; ; attempt++) {
                signal?.throwIfAborted();
                try {
                    const response = await fetch(url, { signal, headers: range ? { Range: range } : undefined });
                    if (!response.ok) throw new Error(`failed to load ${url.split('?')[0]} (${response.status})`);
                    return await decode(response);
                } catch (error) {
                    // fetch/body transport failures are safe to retry for immutable GETs.
                    // Invalid artifacts and HTTP errors remain visible to callers.
                    if (signal?.aborted || !(error instanceof TypeError) || attempt >= 2) throw error;
                    await new Promise<void>((resolve, reject) => {
                        const abort = () => { clearTimeout(timer); reject(signal?.reason); };
                        const timer = setTimeout(() => { signal?.removeEventListener('abort', abort); resolve(); }, 100 * (attempt + 1));
                        signal?.addEventListener('abort', abort, { once: true });
                    });
                }
            }
        } finally {
            this.activeReads--;
            this.waitingReads.shift()?.();
        }
    }
}

/** Blocks grouped by pack and position, a group closing where the next block lies too far ahead. */
function coalesce(references: ArtifactRef[]): ArtifactRef[][] {
    const sorted = [...references].sort((left, right) => left.pack - right.pack || left.offset - right.offset);
    const groups: ArtifactRef[][] = [];
    let end = 0;
    for (const reference of sorted) {
        const group = groups.at(-1);
        if (group && group[0]!.pack === reference.pack && reference.offset <= end + COALESCE_GAP_BYTES) {
            group.push(reference);
            end = Math.max(end, reference.offset + reference.bytes);
        } else {
            groups.push([reference]);
            end = reference.offset + reference.bytes;
        }
    }
    return groups;
}
