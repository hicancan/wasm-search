import { readFile } from 'node:fs/promises';
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
    const bundle = compile(documents.map(document => JSON.stringify(document)).join('\n'), '{}');
    const manifest = JSON.parse(bundle.manifest_json());
    const files = new Map(Array.from({ length: bundle.file_count() }, (_, index) => [bundle.file_path(index), bundle.file_bytes(index)]));
    const engine = new SearchEngine(files.get(manifest.documents.path)!, manifest.documents.decoded_bytes,
        files.get(manifest.lexicon.path)!, manifest.lexicon.decoded_bytes, '{}');
    try {
        for (const [query, expected] of [['tushuguan', 'library'], ['tu shu guan', 'library'],
            ['tsg', 'literal'], ['JIAO3', 'teaching'], ['j3', 'teaching'], ['图书馆', 'library']]) {
            for (const chunk of JSON.parse(engine.begin_search(JSON.stringify({ query })))) {
                const reference = manifest.postings[chunk];
                engine.load_postings_chunk(chunk, files.get(reference.path)!, reference.decoded_bytes);
            }
            const shells = JSON.parse(engine.prepare_search());
            expect(shells.results[0].id, query).toBe(expected);
            if (query === 'tsg') expect(shells.results[1].id).toBe('library');
            for (const chunk of JSON.parse(engine.required_content_chunks(0, 10))) {
                const reference = manifest.content[chunk];
                engine.load_content_chunk(chunk, files.get(reference.path)!, reference.decoded_bytes);
            }
            const result = engine.hydrate_search(0, 10);
            expect(JSON.parse(result).results[0].id, query).toBe(expected);
            expect(result).not.toContain('py:');
        }
    } finally {
        engine.free();
        bundle.free();
    }
});
