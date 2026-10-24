import { afterEach, describe, expect, it, vi } from 'vitest';

import { SearchClient } from './client';
import type { SearchWorkerRequest, SearchWorkerResponse } from './protocol';

class FakeWorker {
    static instances: FakeWorker[] = [];

    onmessage: ((event: MessageEvent<SearchWorkerResponse>) => void) | null = null;
    onerror: ((event: ErrorEvent) => void) | null = null;
    readonly messages: SearchWorkerRequest[] = [];
    terminated = false;

    constructor() {
        FakeWorker.instances.push(this);
    }

    postMessage(message: SearchWorkerRequest): void {
        this.messages.push(message);
    }

    terminate(): void {
        this.terminated = true;
    }

    emit(message: SearchWorkerResponse): void {
        this.onmessage?.({ data: message } as MessageEvent<SearchWorkerResponse>);
    }
}

afterEach(() => {
    FakeWorker.instances = [];
    vi.unstubAllGlobals();
});

describe('SearchClient', () => {
    it('starts no worker until it is first used', () => {
        vi.stubGlobal('Worker', FakeWorker);
        const client = new SearchClient({ baseUrl: '/search' });
        expect(FakeWorker.instances).toHaveLength(0);
        void client.initialize().catch(() => undefined);
        expect(FakeWorker.instances).toHaveLength(1);
        client.dispose();
    });

    it('delivers stable ranked shells before resolving hydrated results', async () => {
        vi.stubGlobal('Worker', FakeWorker);
        const client = new SearchClient({ baseUrl: '/explicit/search' });
        const ready = client.initialize();
        const worker = FakeWorker.instances[0];
        if (!worker) throw new Error('SearchClient did not create its Worker');
        worker.emit({
            type: 'ready',
            documentCount: 12,
            filterOptions: { sources: [], categories: [], categoriesBySource: {} },
        });
        await ready;
        const onUpdate = vi.fn();
        const pending = client.search({
            query: '计算机等级',
            limit: 10,
            sort: 'relevance',
            filters: {},
        }, onUpdate);
        await Promise.resolve();
        const ranked = { totalCandidates: 3, results: [] };
        worker.emit({
            type: 'results',
            requestId: pending.requestId,
            stage: 'ranked',
            response: ranked,
        });
        expect(onUpdate).toHaveBeenCalledWith(ranked);
        const hydrated = { totalCandidates: 3, results: [] };
        worker.emit({
            type: 'results',
            requestId: pending.requestId,
            stage: 'hydrated',
            response: hydrated,
        });
        await expect(pending.response).resolves.toBe(hydrated);
        client.dispose();
    });

    it('owns initialization, cancellation, pending requests, and disposal', async () => {
        vi.stubGlobal('Worker', FakeWorker);
        const client = new SearchClient({
            baseUrl: '/explicit/search',
            cacheBudgetBytes: 1024,
            workingSetBudgetBytes: 512,
        });
        const ready = client.initialize();
        const worker = FakeWorker.instances[0];
        if (!worker) throw new Error('SearchClient did not create its Worker');
        expect(worker.messages).toEqual([{
            type: 'init',
            baseUrl: '/explicit/search',
            cacheBudgetBytes: 1024,
            workingSetBudgetBytes: 512,
        }]);
        worker.emit({
            type: 'ready',
            documentCount: 12,
            filterOptions: { sources: [], categories: [], categoriesBySource: {} },
        });
        await expect(ready).resolves.toMatchObject({ documentCount: 12 });

        const pending = client.search({
            query: '转专业',
            limit: 10,
            sort: 'relevance',
            filters: {},
        });
        await Promise.resolve();
        client.cancel(pending.requestId);
        await expect(pending.response).rejects.toMatchObject({ name: 'AbortError' });
        expect(worker.messages[worker.messages.length - 1]).toEqual({
            type: 'cancel',
            requestId: pending.requestId,
        });

        client.dispose();
        expect(worker.terminated).toBe(true);
        expect(() => client.search({
            query: '奖学金',
            limit: 10,
            sort: 'relevance',
            filters: {},
        })).toThrow('SearchClient is disposed');
    });

    it('can retry failed initialization and rejects readiness on disposal', async () => {
        vi.stubGlobal('Worker', FakeWorker);
        const client = new SearchClient({ baseUrl: '/explicit/search' });
        const first = client.initialize();
        const worker = FakeWorker.instances[0];
        if (!worker) throw new Error('SearchClient did not create its Worker');
        worker.emit({ type: 'error', message: 'bad manifest' });
        await expect(first).rejects.toThrow('bad manifest');

        const second = client.initialize();
        expect(worker.messages.filter(message => message.type === 'init')).toHaveLength(2);
        client.dispose();
        await expect(second).rejects.toThrow('disposed');
    });

    it('rejects every pending request when its owned worker fails', async () => {
        vi.stubGlobal('Worker', FakeWorker);
        const client = new SearchClient({ baseUrl: '/explicit/search' });
        const ready = client.initialize();
        const worker = FakeWorker.instances[0];
        if (!worker) throw new Error('SearchClient did not create its Worker');
        const pending = client.search({
            query: '期末考试',
            limit: 10,
            sort: 'relevance',
            filters: {},
        });

        worker.onerror?.({ message: 'worker crashed' } as ErrorEvent);

        await expect(ready).rejects.toThrow('worker crashed');
        await expect(pending.response).rejects.toThrow('worker crashed');
        client.dispose();
    });

    it('reinitializes the owned runtime and rejects in-flight work', async () => {
        vi.stubGlobal('Worker', FakeWorker);
        const client = new SearchClient({ baseUrl: '/explicit/search' });
        const firstReady = client.initialize();
        const worker = FakeWorker.instances[0];
        if (!worker) throw new Error('SearchClient did not create its Worker');
        worker.emit({
            type: 'ready',
            documentCount: 12,
            filterOptions: { sources: [], categories: [], categoriesBySource: {} },
        });
        await firstReady;

        const pending = client.search({
            query: '转专业',
            limit: 10,
            sort: 'relevance',
            filters: {},
        });
        await Promise.resolve();
        const secondReady = client.reinitialize();
        await expect(pending.response).rejects.toMatchObject({ name: 'AbortError' });
        expect(worker.messages.slice(-2)).toEqual([
            { type: 'cancel', requestId: pending.requestId },
            {
                type: 'init',
                baseUrl: '/explicit/search',
                cacheBudgetBytes: 64 * 1024 * 1024,
                workingSetBudgetBytes: 48 * 1024 * 1024,
            },
        ]);

        worker.emit({
            type: 'ready',
            documentCount: 13,
            filterOptions: { sources: [], categories: [], categoriesBySource: {} },
        });
        await expect(secondReady).resolves.toMatchObject({
            documentCount: 13,
        });
        client.dispose();
    });
});
