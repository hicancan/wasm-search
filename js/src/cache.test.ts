import { describe, expect, it } from 'vitest';
import { CacheStore } from './cache';

describe('CacheStore', () => {
    it('evicts least recently used artifacts within its byte budget', () => {
        const cache = new CacheStore(8);
        cache.set('a', new ArrayBuffer(4));
        cache.set('b', new ArrayBuffer(4));
        expect(cache.get('a')).toBeDefined();

        cache.set('c', new ArrayBuffer(4));

        expect(cache.get('a')).toBeDefined();
        expect(cache.get('b')).toBeUndefined();
        expect(cache.get('c')).toBeDefined();
        expect(cache.sizeBytes).toBe(8);
    });

    it('rejects an artifact larger than the instance budget', () => {
        const cache = new CacheStore(4);
        expect(() => cache.set('large', new ArrayBuffer(5))).toThrow(
            'artifact exceeds cache budget',
        );
    });
});
