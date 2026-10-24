// Build a search bundle in Node: wasm-search/compile.
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import initWasm, { compile } from '../wasm/wasm_search_wasm.js';
import type { Profile, SearchDocument } from './model.js';

let ready: Promise<unknown> | null = null;

/** Compile documents into a bundle directory, replacing whatever was there. */
export async function compileBundle(
    documents: Iterable<SearchDocument>,
    profile: Profile,
    outDir: string,
): Promise<{ documents: number; files: number; bytes: number }> {
    ready ??= initWasm({ module_or_path: await readFile(new URL('../wasm/wasm_search_wasm_bg.wasm', import.meta.url)) });
    await ready;
    let jsonl = '';
    let count = 0;
    for (const document of documents) {
        jsonl += JSON.stringify(document) + '\n';
        count++;
    }
    const bundle = compile(jsonl, JSON.stringify(profile));
    try {
        await rm(outDir, { recursive: true, force: true });
        await mkdir(outDir, { recursive: true });
        let bytes = 0;
        for (let index = 0; index < bundle.file_count(); index++) {
            const data = bundle.file_bytes(index);
            bytes += data.byteLength;
            await writeFile(join(outDir, bundle.file_path(index)), data);
        }
        await writeFile(join(outDir, 'manifest.json'), bundle.manifest_json());
        return { documents: count, files: bundle.file_count() + 1, bytes };
    } finally {
        bundle.free();
    }
}
