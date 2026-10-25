# wasm-search

Full-text search that runs entirely in the browser. A Rust engine — CJK-aware
analysis, an inverted index, tiered ranking and snippets — compiled to
WebAssembly. Bundles are built in Node (or with the CLI) from a JSON-lines file
and served as static files; the browser downloads only the chunks a query needs.

**Production Deployment & Live Demo:** [https://studev.hicancan.top/search/](https://studev.hicancan.top/search/)

```text
documents.jsonl + profile.json ──compile──► bundle/ ──static hosting──► SearchClient (Worker + WASM)
```

## Documents

One JSON object per line:

```json
{"id": "a1", "source": "news", "source_name": "News", "url": "https://…", "title": "…",
 "text": "…", "published": "2026-07-01", "section": "…", "kind": "page",
 "category": "notice", "tags": ["…"], "files": [{"name": "form.pdf", "url": "https://…", "ext": "pdf"}]}
```

`kind` is `page`, `file` or `link`. `category` is any string you want to
filter by.

## Profile

Corpus knowledge travels inside the bundle, so every query applies it:

```json
{"aliases": [{"terms": ["cet"], "phrases": ["college english test"], "prefer_sources": ["exams"]}]}
```

A query equal to one of `terms` searches `phrases` instead and ranks documents
from `prefer_sources` first within a relevance tier.

## Build a bundle

```js
import { compileBundle } from 'wasm-search/compile';
await compileBundle(documents, profile, 'public/search');
```

or `wasm-search build --documents docs.jsonl --profile profile.json --out public/search`.

## Search in the browser

```js
import { SearchClient } from 'wasm-search';
const client = new SearchClient({ baseUrl: '/search' });
await client.initialize();
const { totalCandidates, results } = await client.search(
  { query: '转专业', limit: 10, sort: 'relevance', filters: { category: 'notice' } },
  ranked => render(ranked),   // exact order first, snippets follow
);
```

Bundles are immutable: serve each build from its own directory and cache it
forever.

## Repository

```text
core/   analysis, bundle codec, ranking (the only search semantics)
wasm/   WebAssembly bindings: SearchEngine and compile()
cli/    wasm-search build / query
js/     npm package: SearchClient (browser) and compileBundle (Node)
```

```sh
cargo test --workspace
cd js && npm install && npm run build && npm test
```

The npm package is published as a tarball on each GitHub release.

Licensed under AGPL-3.0.
