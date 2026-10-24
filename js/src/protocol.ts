import type { FilterOptions, Query, SearchResponse } from './model.js';

export type SearchWorkerRequest =
    | {
        type: 'init';
        baseUrl: string;
        cacheBudgetBytes: number;
        workingSetBudgetBytes: number;
    }
    | {
        type: 'search';
        requestId: number;
        query: Query;
    }
    | {
        type: 'cancel';
        requestId: number;
    };

export type SearchWorkerResponse =
    | {
        type: 'ready';
        documentCount: number;
        filterOptions: FilterOptions;
    }
    | {
        type: 'results';
        requestId: number;
        stage: 'ranked' | 'hydrated';
        response: SearchResponse;
    }
    | {
        type: 'error';
        requestId?: number;
        message: string;
    };
