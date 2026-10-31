# wasm-search v2.0.0

This release changes the bundle format to `wasm-search-bundle-v2` and removes
the eager whole-lexicon reader. The lexicon is front-coded into fixed target
64 KiB blocks. `lexicon-index.bin` contains the first key of each block; a
query loads only the blocks selected by binary search, then asks the same host
fetch callback for the required postings. A one-block lexicon uses the same
layout.

The v2 compiler emits `lexicon.index` and `lexicon.blocks` in the manifest.
The old `lexicon.bin` format is intentionally unsupported. The package and all
Rust crates are version `2.0.0`.

The checked-in StuDev search inputs produced 104,549,871 artifact bytes in v2
versus 104,571,975 bytes in v1.0.3 (-0.02%). The representative `转专业`
query fetched 62,004 bytes of lexicon blocks and index in v2 versus the
15,859,316-byte v1 lexicon. The full fresh-session measurement still includes
the existing 2.5 MB documents artifact; that artifact is unchanged by this
release.

## Host migration

JavaScript:

- Require `manifest.format === "wasm-search-bundle-v2"`.
- Replace `manifest.lexicon` with `manifest.lexicon.index` and
  `manifest.lexicon.blocks`.
- Construct the wasm engine with the compressed index artifact.
- After `beginSearch`, fetch/cache the returned lexicon block numbers and call
  `loadLexiconBlock`; then call `requiredPostingChunks` and load those posting
  artifacts before `prepareSearch`.
- Delete any `lexicon.bin` fetch, size check, or cache key.

Kotlin/JNI:

- Pass the lexicon index bytes and decoded size to `SearchEngine`.
- For each block returned by `beginSearch`, fetch it and call
  `loadLexiconBlock(block, bytes, decodedBytes)`.
- Call `requiredPostingChunks()` and continue with the existing postings,
  content, `prepareSearch`, and `hydrateSearch` flow.
- Replace any `lexicon.bin` asset lookup with the manifest index/blocks.

## Local Brotli benchmark

The table uses a fresh Chromium profile per query, a local static server, and
the same checked-in StuDev search inputs. Bytes are response `Content-Length`
values; `v2 lex` is the index plus fetched lexicon blocks. The v1 server uses
the released whole-lexicon artifact.

| query | v1 bytes | v2 bytes | v1 ms | v2 ms | v2 lex |
| --- | ---: | ---: | ---: | ---: | ---: |
| 转专业 | 20,407,838 | 4,640,412 | 1,354 | 329 | 62,004 |
| 奖学金 | 20,330,373 | 4,565,802 | 1,335 | 322 | 64,360 |
| 计算机等级 | 22,823,074 | 7,292,788 | 1,329 | 822 | 323,496 |
| 图书馆 | 19,962,502 | 4,200,415 | 1,350 | 336 | 64,019 |
| 课表 | 19,928,901 | 4,141,242 | 1,348 | 232 | 39,232 |
| 课程 | 19,769,314 | 3,981,627 | 1,350 | 325 | 39,232 |
| 考试安排 | 20,346,670 | 4,606,665 | 1,350 | 334 | 85,037 |
| 空教室 | 20,126,479 | 4,363,424 | 1,339 | 327 | 66,288 |
| 教3-601 | 19,903,185 | 4,151,413 | 1,346 | 342 | 71,604 |
| 教3 | 19,210,282 | 3,425,761 | 1,333 | 206 | 40,865 |
| tushuguan | 19,581,038 | 3,832,785 | 1,365 | 316 | 77,632 |
| tsg | 20,049,027 | 4,310,807 | 1,352 | 324 | 83,032 |
| jiao3 | 19,710,515 | 3,965,465 | 1,337 | 846 | 77,606 |
| cet | 19,958,411 | 4,217,426 | 1,358 | 325 | 78,483 |
| python | 19,795,199 | 4,048,864 | 1,336 | 321 | 78,013 |
| network | 19,999,697 | 4,256,483 | 1,336 | 219 | 79,665 |
| 2026 | 19,779,577 | 3,921,187 | 1,335 | 340 | 74,335 |
| 2025级 | 19,937,183 | 4,155,040 | 1,330 | 324 | 43,544 |
| 4号楼 | 20,140,471 | 4,380,731 | 1,336 | 217 | 68,756 |
| 电费 | 19,880,547 | 4,100,112 | 1,355 | 834 | 38,924 |

The ranking-equivalence test compares the fixed query set (`转专业`, `学校`,
`新闻`, `不存在`) between an eager v2 reader and the staged lazy reader. The
top-k document IDs are identical; the Android tests also exercise the same
staged protocol.
