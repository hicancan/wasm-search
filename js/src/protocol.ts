import type { FilterOptions, Query, SearchResponse } from './model.js';
import type { SearchBundleManifest } from './artifacts.js';

export type SearchWorkerRequest =
    | {
        type: 'init';
        baseUrl: string;
        cacheBudgetBytes: number;
        workingSetBudgetBytes: number;
        manifest?: SearchBundleManifest;
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
        response: SearchResponse;
    }
    | {
        type: 'error';
        requestId?: number;
        message: string;
    };
