# wasm-search v2.0.0

This release changes the bundle format to `wasm-search-bundle-v2` and removes
the eager whole-lexicon reader. The lexicon is front-coded into fixed target
128 KiB blocks. `lexicon-index.bin` contains the first key of each block; a
query loads only the blocks selected by binary search, then asks the same host
fetch callback for the required postings. A one-block lexicon uses the same
layout.

The v2 compiler emits `lexicon.index` and `lexicon.blocks` in the manifest.
The old `lexicon.bin` format is intentionally unsupported. The package and all
Rust crates are version `2.0.0`.

The checked-in StuDev search inputs produced 106,024,021 artifact bytes in v2
versus 104,571,975 bytes in v1.0.3 (+1.39%). The document index is loaded with
the lexicon index; titles, URLs, source labels, dates, sections, and attachments
are in fixed 16 KiB document blocks. Ranking requests only the blocks containing
the score candidates, while filtered and date-sorted requests request the full
candidate set so their results remain exact.

## Host migration

JavaScript:

- Require `manifest.format === "wasm-search-bundle-v2"`.
- Replace `manifest.lexicon` with `manifest.lexicon.index` and
  `manifest.lexicon.blocks`.
- Replace `manifest.documents` with `manifest.documents.index` and
  `manifest.documents.blocks`.
- Construct the wasm engine with the compressed index artifact.
- After `beginSearch`, fetch/cache the returned lexicon block numbers and call
  `loadLexiconBlock`; then call `requiredPostingChunks` and load those posting
  artifacts. Call `requiredDocumentBlocks`, fetch/cache those document blocks,
  and call `loadDocumentBlock` before `prepareSearch`.
- Delete any `documents.bin` or `lexicon.bin` fetch, size check, or cache key.

Kotlin/JNI:

- Pass the lexicon index bytes and decoded size to `SearchEngine`.
- For each block returned by `beginSearch`, fetch it and call
  `loadLexiconBlock(block, bytes, decodedBytes)`.
- Call `requiredPostingChunks()` and load the requested postings. Then call
  `requiredDocumentBlocks()`, load each returned document block with
  `loadDocumentBlock(block, bytes, decodedBytes)`, and continue with
  `prepareSearch`, content, and `hydrateSearch`.
- Replace any `documents.bin` or `lexicon.bin` asset lookup with the manifest
  index/blocks.

## Local Brotli benchmark

The table uses a fresh Chromium profile per query, a local static server, and
the same checked-in StuDev search inputs. Bytes are response `Content-Length`
values; `v2 lex` is the index plus fetched lexicon blocks. The v1 server uses
the released whole-lexicon artifact.

| query | v1 bytes | v2 bytes | v1 ms | v2 ms |
| --- | ---: | ---: | ---: | ---: |
| 转专业 | 20,407,838 | 279,357 | 1,354 | 518 |
| 奖学金 | 20,330,373 | 305,393 | 1,335 | 348 |
| 计算机等级 | 22,823,074 | 463,045 | 1,329 | 825 |
| 图书馆 | 19,962,502 | 295,482 | 1,350 | 618 |
| 课表 | 19,928,901 | 178,145 | 1,348 | 296 |
| 课程 | 19,769,314 | 181,502 | 1,350 | 303 |
| 考试安排 | 20,346,670 | 420,585 | 1,350 | 699 |
| 空教室 | 20,126,479 | 293,036 | 1,339 | 499 |
| 教3-601 | 19,903,185 | 217,655 | 1,346 | 365 |
| 教3 | 19,210,282 | 58,691 | 1,333 | 34 |
| tushuguan | 19,581,038 | 217,381 | 1,365 | 383 |
| tsg | 20,049,027 | 296,793 | 1,352 | 524 |
| jiao3 | 19,710,515 | 243,369 | 1,337 | 419 |
| cet | 19,958,411 | 245,867 | 1,358 | 496 |
| python | 19,795,199 | 236,471 | 1,336 | 422 |
| network | 19,999,697 | 240,283 | 1,336 | 514 |
| 2026 | 19,779,577 | 233,125 | 1,335 | 417 |
| 2025级 | 19,937,183 | 161,161 | 1,330 | 354 |
| 4号楼 | 20,140,471 | 228,013 | 1,336 | 498 |
| 电费 | 19,880,547 | 161,298 | 1,355 | 305 |

The v2 column is the wire `Content-Length` before the first result, excluding
wasm, from a fresh session against a local Brotli static server. It includes the
document index, selected document blocks, lexicon blocks, and postings. The
median is 238,377 bytes; the few broad multi-posting queries exceed 300 KB.
The ranking-equivalence test compares the fixed query set (`转专业`, `学校`,
`新闻`, `不存在`) between an eager v2 reader and the staged lazy reader. The
top-k document IDs are identical; the Android tests also exercise the same
staged protocol.
