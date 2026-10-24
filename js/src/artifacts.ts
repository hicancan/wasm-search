import { CacheStore } from './cache';
import type { Profile } from './model';

export interface ArtifactRef {
    path: string;
    bytes: number;
    decoded_bytes: number;
}

export interface SearchBundleManifest {
    format: 'wasm-search-bundle-v1';
    documents: ArtifactRef;
    lexicon: ArtifactRef;
    postings: ArtifactRef[];
    content: ArtifactRef[];
    profile: Profile;
}

function parseManifest(value: unknown): SearchBundleManifest {
    const manifest = value as Partial<SearchBundleManifest> | null;
    if (!manifest || manifest.format !== 'wasm-search-bundle-v1') {
        throw new Error('unsupported search bundle format');
    }
    if (!manifest.documents || !manifest.lexicon || !manifest.postings?.length || !manifest.content?.length) {
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

    constructor(baseUrl: string, cache: CacheStore) {
        this.baseUrl = baseUrl.replace(/\/+$/, '');
        this.cache = cache;
    }

    manifest(signal?: AbortSignal): Promise<SearchBundleManifest> {
        this.manifestRequest ??= (async () => {
            const response = await fetch(`${this.baseUrl}/manifest.json`, { signal });
            if (!response.ok) throw new Error(`failed to load search manifest (${response.status})`);
            return parseManifest(await response.json());
        })().catch(error => {
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
        const request = (async () => {
            const response = await fetch(`${this.baseUrl}/${reference.path}`, { signal });
            if (!response.ok) throw new Error(`failed to load ${reference.path} (${response.status})`);
            const bytes = await response.arrayBuffer();
            if (bytes.byteLength !== reference.bytes) throw new Error(`truncated ${reference.path}`);
            this.cache.set(reference.path, bytes);
            return bytes;
        })().finally(() => this.requests.delete(reference.path));
        this.requests.set(reference.path, request);
        return request;
    }
}
