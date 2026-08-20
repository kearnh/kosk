# Text completion (plan)

Design notes, crate survey, and intended keyboard UX for the text-input prediction engine. This is not a description of wired UI; the engine itself is explained in [../completion.md](../completion.md). See also [todo.md](../../todo.md).

## Crate survey (v0 decision)

| Crate / approach | Role | Verdict for v0 |
|------------------|------|----------------|
| **`fst`** | Memory-mapped finite state transducer; excellent prefix maps on huge static sets | Deferred: adds a dependency and build-time index step; overkill until wordlists are large. |
| **Hunspell / spellcheck crates** | Morphology-aware correction | Deferred: dict licensing and FFI complexity; better when typo correction is a goal. |
| **SymSpell-style** | Fast fuzzy prefix / edit distance | Deferred: v0 is exact prefix only. |
| **Sorted `Vec<String>` + binary search** | `partition_point` / lower bound on first string `>= token`, scan forward while `starts_with(token)` | **Chosen for v0**: few dependencies, obvious behavior, easy to test. |
| **Drop-in “predictive keyboard”** | — | None found that fits egui + this repo’s controller model; a thin `CompletionEngine` trait stays the integration point. |

**Chosen v0:** in-crate `DictionaryEngine` backed by a sorted, deduplicated word list, exact prefix match, capped results. Heavier models (ONNX, n-grams) are out of scope until dictionary UX is proven.

## Design notes (literature skim)

- **Current token:** suggestions use the alphanumeric suffix of `prefix` that **ends at the cursor** only if the cursor is not in whitespace (e.g. `"hello "` → no token; `"hello wor"` → `wor`).
- **Prefix vs suffix:** `suffix` is passed through `CompletionContext` for future engines (e.g. morphological completion, closing quotes); v0 dictionary ignores it.
- **Ranking:** v0 uses lexicographic order from the sorted list; later: frequency, recency, or keyboard distance.
- **T9 / disambiguation:** multi-letter-key disambiguation is a different UX layer; not mixed into this engine.
- **Typo tolerance:** SymSpell / edit distance can plug in as another `CompletionEngine` without changing the trait.
- **Empty token:** v0 returns no suggestions (no “frequent words” list yet).
- **Debouncing / async:** UI will debounce when wired; the engine API stays synchronous.
- **Privacy:** offline wordlists only unless explicitly extended later.

## `completion_dev` binary

Run from the repo root:

```text
cargo run --bin completion_dev -- --text "hello wor" --cursor 9
```

- `--text` — full buffer (UTF-8). `--cursor` — **byte** index into `text`, must lie on a UTF-8 character boundary (same convention as the text field caret).
- `--max` — max suggestions (default `8`).
- `--wordlist PATH` — optional path to a newline-separated word list (UTF-8). If omitted, a small built-in list is used for demos.

Example:

```text
cargo run --bin completion_dev -- --text "hel" --cursor 3
```

## Library API

Rust entry: `kosk::completion` (`DictionaryEngine`, `CompletionEngine`, `CompletionContext`, `Suggestion`, `split_at_cursor`, `word_prefix_token`). Unit tests: `cargo test completion`.
