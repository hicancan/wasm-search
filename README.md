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

## Pinyin

Bundle compilation uses a generic, plain-syllable character dictionary to add
full pinyin and initials to the inverted index. For example, `图书馆` matches
`tushuguan`, `tu shu guan` and `tsg`; `教3` matches `jiao3` and `j3`.
Queries are case insensitive, and `ü` can also be entered as `v`. Literal title matches retain higher relevance
than transliterations, and snippets never expose internal transliteration terms.
No corpus aliases or client-side title scans are required; both WASM and Android
consume the same compiled postings. Recompile existing bundles to enable pinyin.

The table uses one default pronunciation per character, without contextual
polyphone disambiguation. Chinese windows retain the normal index limits
(four title characters / two body characters), plus a complete name up to 32
characters. This bounds index growth for long prose while supporting full names.

## Android

With Rust's Android targets, cargo-ndk, the Android NDK and JDK installed, run
`pwsh android/build.ps1` (toolchain paths can be set with `-NdkPath` and
`-JdkPath`). It builds release libraries for `arm64-v8a` and `x86_64` in
`android/build/jniLibs` (`-OutputDirectory` overrides this) and generates Kotlin
bindings in `android/build/kotlin`, package `top.hicancan.search`. Add these to
your app's `jniLibs` and Kotlin sources, with JNA's Android AAR on the classpath.
The engine uses the same bundle artifacts and JSON search protocol as wasm;
call `beginSearch`, load its postings, `prepareSearch`, then load
`requiredContentChunks` and call `hydrateSearch` for each page.

## Repository

```text
core/    analysis, bundle codec, ranking (the only search semantics)
wasm/    WebAssembly bindings: SearchEngine and compile()
android/ UniFFI bindings and Android native library build
cli/     wasm-search build / query
js/      npm package: SearchClient (browser) and compileBundle (Node)
```

```sh
cargo test --workspace
cd js && npm install && npm run build && npm test
```

The npm package is published as a tarball on each GitHub release.

Licensed under AGPL-3.0.
