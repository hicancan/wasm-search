import { readFile } from 'node:fs/promises';
import { zstdCompressSync } from 'node:zlib';
import { beforeAll, expect, it } from 'vitest';
import initialize, { compile, SearchEngine } from '../wasm/wasm_search_wasm.js';

beforeAll(async () => {
    await initialize({ module_or_path: await readFile(new URL('../wasm/wasm_search_wasm_bg.wasm', import.meta.url)) });
});

it('compiles pinyin once and searches full, spaced, initial and mixed digit queries in actual WASM', () => {
    const documents = [
        { id: 'library', title: '图书馆' },
        { id: 'teaching', title: '教3' },
        { id: 'literal', title: 'tsg' },
    ].map(document => ({ ...document, source: 'places', source_name: 'Places', url: `https://example.test/${document.id}`, text: 'Generic fixture' }));
    const bundle = compile(documents.map(document => JSON.stringify(document)).join('\n'), '{}', (bytes: Uint8Array) => zstdCompressSync(bytes));
    const manifest = JSON.parse(bundle.manifest_json());
    const packs = Array.from({ length: bundle.file_count() }, (_, index) => bundle.file_bytes(index));
    const read = (block: { pack: number; offset: number; bytes: number }) => packs[block.pack]!.slice(block.offset, block.offset + block.bytes);
    const engine = new SearchEngine(read(manifest.index), manifest.index.decoded_bytes, '{}');
    try {
        for (const [query, expected] of [['tushuguan', 'library'], ['tu shu guan', 'library'],
            ['tsg', 'literal'], ['JIAO3', 'teaching'], ['j3', 'teaching'], ['图书馆', 'library']]) {
            for (const block of JSON.parse(engine.begin_search(JSON.stringify({ query })))) engine.load_lexicon_block(block.id, read(block), block.decoded_bytes);
            for (const block of JSON.parse(engine.required_posting_chunks())) engine.load_postings_chunk(block.id, read(block), block.decoded_bytes);
            for (const block of JSON.parse(engine.plan_search())) engine.load_document_block(block.id, read(block), block.decoded_bytes);
            const shells = JSON.parse(engine.ranked_results());
            expect(shells.results[0].id, query).toBe(expected);
            if (query === 'tsg') expect(shells.results[1].id).toBe('library');
            for (const block of JSON.parse(engine.required_content_chunks())) engine.load_content_chunk(block.id, read(block), block.decoded_bytes);
            const result = engine.hydrate_search();
            expect(JSON.parse(result).results[0].id, query).toBe(expected);
            expect(result).not.toContain('py:');
        }
    } finally {
        engine.free();
        bundle.free();
    }
});
