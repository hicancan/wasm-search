/// <reference lib="webworker" />

import { ArtifactSource } from './artifacts';
import { CacheStore } from './cache';
import { SearchRuntime } from './runtime';
import type { SearchWorkerRequest, SearchWorkerResponse } from './protocol';

const scope = self as DedicatedWorkerGlobalScope;
let runtime: SearchRuntime | null = null;
let initialization: AbortController | null = null;
const searches = new Map<number, AbortController>();
let queue = Promise.resolve();

function post(message: SearchWorkerResponse): void {
    scope.postMessage(message);
}

async function handle(request: Exclude<SearchWorkerRequest, { type: 'cancel' }>): Promise<void> {
    if (request.type === 'init') {
        initialization?.abort();
        for (const controller of searches.values()) controller.abort();
        searches.clear();
        runtime?.dispose();

        const controller = new AbortController();
        initialization = controller;
        const next = new SearchRuntime(
            new ArtifactSource(
                request.baseUrl,
                new CacheStore(request.cacheBudgetBytes),
            ),
            request.workingSetBudgetBytes,
        );
        runtime = next;
        try {
            const { documentCount, filterOptions } = await next.initialize(controller.signal);
            if (!controller.signal.aborted && runtime === next) {
                post({
                    type: 'ready',
                    documentCount,
                    filterOptions,
                });
            }
        } catch (error) {
            if (!controller.signal.aborted) {
                runtime = null;
                post({
                    type: 'error',
                    message: error instanceof Error ? error.message : String(error),
                });
            }
        }
        return;
    }

    if (!runtime) {
        post({
            type: 'error',
            requestId: request.requestId,
            message: 'search worker is not initialized',
        });
        return;
    }
    const controller = searches.get(request.requestId);
    if (!controller || controller.signal.aborted) return;
    try {
        const response = await runtime.search(request.query, ranked => {
            if (!controller.signal.aborted) {
                post({
                    type: 'results',
                    requestId: request.requestId,
                    stage: 'ranked',
                    response: ranked,
                });
            }
        }, controller.signal);
        if (!controller.signal.aborted) {
            post({
                type: 'results',
                requestId: request.requestId,
                stage: 'hydrated',
                response,
            });
        }
    } catch (error) {
        if (!controller.signal.aborted) {
            post({
                type: 'error',
                requestId: request.requestId,
                message: error instanceof Error ? error.message : String(error),
            });
        }
    } finally {
        if (searches.get(request.requestId) === controller) {
            searches.delete(request.requestId);
        }
    }
}

scope.onmessage = (event: MessageEvent<SearchWorkerRequest>) => {
    const request = event.data;
    if (request.type === 'cancel') {
        searches.get(request.requestId)?.abort();
        searches.delete(request.requestId);
        return;
    }
    if (request.type === 'search') {
        for (const controller of searches.values()) controller.abort();
        searches.clear();
        searches.set(request.requestId, new AbortController());
    }
    queue = queue
        .then(() => handle(request))
        .catch(error => {
            post({
                type: 'error',
                message: error instanceof Error ? error.message : String(error),
            });
        });
};
