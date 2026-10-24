# wasm-search development rules

- This is a generic search engine. It knows nothing about any particular
  corpus; corpus knowledge (aliases, preferred sources, categories) arrives as
  a `Profile` and as fields on each document.
- Search semantics live only in `core` and are shared by `cli` and `wasm`.
  TypeScript orchestrates fetching, workers and caches; it never tokenizes or
  ranks.
- One bundle format at a time. No hash manifests or identity schemes: callers
  version the directory they serve a bundle from.
- Keep tests next to the code they test.
