# wasm-search

Full-text search that runs entirely in the browser. A Rust engine — CJK-aware
analysis, an inverted index, tiered ranking and snippets — compiled to
WebAssembly. Bundles are built in Node (or with the CLI) from a JSON-lines file
and served as static files; the browser downloads only the byte ranges a query needs.

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
);
```

`offset` pages through one stable ranking; `totalCandidates` counts every match.
`filters.sourceIds` narrows to several sources and `excludedSourceIds` removes
them. Each hydrated result has a `snippet`, the window holding the most different
matched terms, and `titleHighlights` / `snippetHighlights`: ascending `[start, end)`
ranges counted in Unicode scalar values, so every client marks the same characters
without analysing text itself.

Bundles are immutable: serve each build from its own directory and cache it
forever. `SEARCH_BUNDLE_FORMAT` (and `bundleFormat()` in the native and wasm
bindings) names the one format an engine reads, as each bundle's manifest names
the format it was written in: a host that publishes the latter beside its data
can tell an app too old for that data to update rather than fail.

Bundle format v5 has one index and a few pack files. The index is the only read a
client makes before its first query: it holds what ranking needs for every document
(normalized title, source, category, date, kind), the first term of every postings
block and the first document of every page, and where every block lies in the packs.
A query then makes two rounds of byte-range reads: the postings blocks of its terms,
then the pages holding the documents it shows (a page carries each document's
metadata and text, so the results arrive complete, snippets included). The browser
client reads neighbouring blocks of a pack with one range and keeps every block it
has read in Cache Storage under the bundle's address, so a repeated query, or a
reload, reads nothing again. A server that ignores range requests still works, at
the cost of whole packs. Hosts drive this through the engine: `beginSearch` names
the postings blocks, `planSearch` the pages, and `hydrateSearch` returns the results.

## Pinyin

Bundle compilation uses a generic, plain-syllable character dictionary to add
full pinyin and initials to the inverted index. For example, `图书馆` matches
`tushuguan`, `tu shu guan` and `tsg`; `教3` matches `jiao3` and `j3`.
Queries are case insensitive, and `ü` can also be entered as `v`. A title whose
whole transliteration is the query (`tsg` for the place `图书馆`) ranks just below
a literal equal title and above titles that merely contain the query; other
transliterations rank below literal title matches. Snippets never expose internal
transliteration terms.
No corpus aliases or client-side title scans are required; both WASM and Android
consume the same compiled postings. Recompile existing bundles to enable pinyin.

The table uses one default pronunciation per character, without contextual
polyphone disambiguation. Chinese windows retain the normal index limits
(four title characters / two body characters), plus a complete name up to 32
characters. This bounds index growth for long prose while supporting full names.

## Android

With Rust's Android targets, cargo-ndk, the Android NDK and JDK installed, run
`pwsh native/build.ps1` (toolchain paths can be set with `-NdkPath` and
`-JdkPath`). It builds release libraries for `arm64-v8a` and `x86_64` in
`native/build/jniLibs` (and the Windows library in `native/build/windows`) (`-OutputDirectory` overrides this) and generates Kotlin
bindings in `native/build/kotlin`, package `top.hicancan.search`. Add these to
your app's `jniLibs` and Kotlin sources, with JNA's Android AAR on the classpath.
The engine uses the same bundle artifacts and JSON search protocol as wasm;
call `beginSearch` and load the postings blocks it names, call `planSearch` and
load the pages it names, then call `hydrateSearch`.

## Repository

```text
core/    analysis, bundle codec, ranking (the only search semantics)
wasm/    WebAssembly bindings: SearchEngine and compile()
native/  UniFFI bindings and the native libraries for Android and Windows
cli/     wasm-search build / query
js/      npm package: SearchClient (browser) and compileBundle (Node)
```

```sh
cargo test --workspace
cd js && npm install && npm run build && npm test
```

The npm package is published as a tarball on each GitHub release.

Licensed under AGPL-3.0.
