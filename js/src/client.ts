import type { FilterOptions, Query, SearchResponse } from './model';
import type { SearchWorkerRequest, SearchWorkerResponse } from './protocol';

export interface SearchClientReady {
    documentCount: number;
    filterOptions: FilterOptions;
}

interface PendingSearch {
    resolve: (response: SearchResponse) => void;
    reject: (error: Error) => void;
    onUpdate?: (response: SearchResponse) => void;
    posted: boolean;
}

type ClientState = 'idle' | 'initializing' | 'ready' | 'disposed';

const MIB = 1024 * 1024;

export interface SearchClientOptions {
    baseUrl: string;
    cacheBudgetBytes?: number;
    workingSetBudgetBytes?: number;
}

export class SearchClient {
    private readonly worker: Worker;
    private readonly options: Required<SearchClientOptions>;
    private state: ClientState = 'idle';
    private readyPromise: Promise<SearchClientReady> | null = null;
    private readyValue: SearchClientReady | null = null;
    private readyResolve: ((ready: SearchClientReady) => void) | null = null;
    private readyReject: ((error: Error) => void) | null = null;
    private readonly pending = new Map<number, PendingSearch>();
    private nextRequestId = 1;

    constructor(options: SearchClientOptions) {
        const baseUrl = options.baseUrl.trim();
        if (!baseUrl) throw new Error('search bundle base URL is required');
        this.options = {
            baseUrl,
            cacheBudgetBytes: options.cacheBudgetBytes ?? 64 * MIB,
            workingSetBudgetBytes: options.workingSetBudgetBytes ?? 48 * MIB,
        };
        this.worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
        this.worker.onmessage = (event: MessageEvent<SearchWorkerResponse>) => this.receive(event.data);
        this.worker.onerror = event => this.fail(new Error(event.message || 'search worker failed'));
    }

    initialize(): Promise<SearchClientReady> {
        this.requireActive();
        if (this.state === 'ready' && this.readyValue) {
            return Promise.resolve(this.readyValue);
        }
        if (this.state === 'initializing' && this.readyPromise) {
            return this.readyPromise;
        }
        this.state = 'initializing';
        this.readyPromise = new Promise<SearchClientReady>((resolve, reject) => {
            this.readyResolve = resolve;
            this.readyReject = reject;
        });
        this.post({ type: 'init', ...this.options });
        return this.readyPromise;
    }

    reinitialize(): Promise<SearchClientReady> {
        this.requireActive();
        if (this.state === 'initializing') {
            throw new Error('SearchClient is already initializing');
        }
        this.rejectPending(new DOMException('search runtime reinitialized', 'AbortError'));
        this.readyValue = null;
        this.readyPromise = null;
        this.state = 'idle';
        return this.initialize();
    }

    search(
        query: Query,
        onUpdate?: (response: SearchResponse) => void,
    ): { requestId: number; response: Promise<SearchResponse> } {
        this.requireActive();
        const requestId = this.nextRequestId++;
        const response = new Promise<SearchResponse>((resolve, reject) => {
            this.pending.set(requestId, { resolve, reject, onUpdate, posted: false });
        });
        void this.initialize().then(() => {
            const pending = this.pending.get(requestId);
            if (!pending || this.state !== 'ready') return;
            pending.posted = true;
            this.post({ type: 'search', requestId, query });
        }).catch(error => {
            this.pending.get(requestId)?.reject(
                error instanceof Error ? error : new Error(String(error)),
            );
            this.pending.delete(requestId);
        });
        return { requestId, response };
    }

    cancel(requestId: number): void {
        const pending = this.pending.get(requestId);
        if (!pending) return;
        pending.reject(new DOMException('search cancelled', 'AbortError'));
        this.pending.delete(requestId);
        if (pending.posted) {
            this.post({ type: 'cancel', requestId });
        }
    }

    dispose(): void {
        if (this.state === 'disposed') return;
        const error = new Error('SearchClient is disposed');
        this.readyReject?.(error);
        this.readyReject = null;
        this.readyResolve = null;
        this.rejectPending(error);
        this.readyPromise = null;
        this.readyValue = null;
        this.state = 'disposed';
        this.worker.terminate();
    }

    private rejectPending(error: Error): void {
        for (const [requestId, pending] of this.pending) {
            pending.reject(error);
            if (pending.posted) {
                this.post({ type: 'cancel', requestId });
            }
        }
        this.pending.clear();
    }

    private fail(error: Error): void {
        this.readyReject?.(error);
        this.readyReject = null;
        this.readyResolve = null;
        this.readyPromise = null;
        this.readyValue = null;
        if (this.state !== 'disposed') this.state = 'idle';
        this.rejectPending(error);
    }

    private post(message: SearchWorkerRequest): void {
        this.worker.postMessage(message);
    }

    private requireActive(): void {
        if (this.state === 'disposed') throw new Error('SearchClient is disposed');
    }

    private receive(message: SearchWorkerResponse): void {
        if (message.type === 'ready') {
            this.readyValue = message;
            this.state = 'ready';
            this.readyResolve?.(message);
            this.readyResolve = null;
            this.readyReject = null;
            return;
        }
        if (message.type === 'results') {
            const pending = this.pending.get(message.requestId);
            if (message.stage === 'ranked') {
                pending?.onUpdate?.(message.response);
                return;
            }
            pending?.resolve(message.response);
            this.pending.delete(message.requestId);
            return;
        }
        const error = new Error(message.message);
        if (message.requestId === undefined) {
            this.fail(error);
        } else {
            this.pending.get(message.requestId)?.reject(error);
            this.pending.delete(message.requestId);
        }
    }
}
