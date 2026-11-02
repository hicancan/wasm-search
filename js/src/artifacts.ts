import { CacheStore } from './cache.js';
import type { Profile } from './model.js';

export interface ArtifactRef {
    path: string;
    bytes: number;
    decoded_bytes: number;
}

export interface LexiconManifest {
    index: ArtifactRef;
    blocks: ArtifactRef[];
}

export interface DocumentManifest {
    index: ArtifactRef;
    blocks: ArtifactRef[];
}

export interface SearchBundleManifest {
    format: 'wasm-search-bundle-v2';
    documents: DocumentManifest;
    lexicon: LexiconManifest;
    postings: ArtifactRef[];
    content: ArtifactRef[];
    profile: Profile;
}

function parseManifest(value: unknown): SearchBundleManifest {
    const manifest = value as Partial<SearchBundleManifest> | null;
    if (!manifest || manifest.format !== 'wasm-search-bundle-v2') {
        throw new Error('unsupported search bundle format');
    }
    if (!manifest.documents?.index || !manifest.documents.blocks?.length || !manifest.lexicon?.index || !manifest.lexicon.blocks?.length || !manifest.postings?.length || !manifest.content?.length) {
        throw new Error('incomplete search bundle manifest');
    }
    return manifest as SearchBundleManifest;
}

/** Fetches a bundle's files from one directory URL, with an LRU byte cache. */
export class ArtifactSource {
    private readonly baseUrl: string;
    private readonly cache: CacheStore;
    private manifestRequest: Promise<SearchBundleManifest> | null = null;
    private readonly requests = new Map<string, Promise<ArrayBuffer>>();
    private activeReads = 0;
    private readonly waitingReads: Array<() => void> = [];

    constructor(baseUrl: string, cache: CacheStore) {
        this.baseUrl = baseUrl.replace(/\/+$/, '');
        this.cache = cache;
    }

    manifest(signal?: AbortSignal): Promise<SearchBundleManifest> {
        this.manifestRequest ??= this.read('manifest.json', async response => parseManifest(await response.json()), signal).catch(error => {
            this.manifestRequest = null;
            throw error;
        });
        return this.manifestRequest;
    }

    bytes(reference: ArtifactRef, signal?: AbortSignal): Promise<ArrayBuffer> {
        const cached = this.cache.get(reference.path);
        if (cached) return Promise.resolve(cached);
        const active = this.requests.get(reference.path);
        if (active) return active;
        const request = this.read(reference.path, async response => {
            const bytes = await response.arrayBuffer();
            if (bytes.byteLength !== reference.bytes) throw new Error(`truncated ${reference.path}`);
            this.cache.set(reference.path, bytes);
            return bytes;
        }, signal).finally(() => this.requests.delete(reference.path));
        this.requests.set(reference.path, request);
        return request;
    }

    /** Keep the slot until the body is consumed: HTTP/2 streams outlive response headers. */
    private async read<T>(path: string, decode: (response: Response) => Promise<T>, signal?: AbortSignal): Promise<T> {
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
                    const response = await fetch(`${this.baseUrl}/${path}`, { signal });
                    if (!response.ok) throw new Error(`failed to load ${path} (${response.status})`);
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
