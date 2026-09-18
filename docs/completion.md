# Text completion (`completion/`)

Word and next-token prediction for Keyboard and TextInput. UI talks only to the session. Backends never see egui, HID, or `config::get()`.

## Layers

1. **Context** — `(text, cursor_byte)` → current token, `token_range`, previous words, case flags (`context.rs`). Keyboard uses a session-scoped log of keys kosk actually sent; TextInput uses the local buffer and caret.
2. **Session** — debounce, latest-wins worker slot, generation filter, highlight, armed latch (`session.rs`). `None` from a backend means abort (stale generation), not “no suggestions.”
3. **`CompletionBackend`** — `suggest(ctx, abort) -> Option<Vec<Candidate>>`. Factory: `backend_from_config`. Default `ngram`; `fallback = "dictionary"` if model files are missing.
4. **Chips** — reserved strip above the keys (`state/completion_ui.rs`). Cycle never injects. Accept injects.

Config lives in `[completion]` (`src/completion/settings.rs`). Relative paths resolve against the config directory. `completion` is in `TAPE_CONFIG_SKIP`.

## Armed latch (Keyboard)

Starts armed (`start_armed`). Arrow / Home / End / Delete and paste **disarm**: stop logging, hide chips, stop predicting. Stays off until `toggleCompletion`. Re-arm clears the log (`clear_log_on_arm`). Enter does not re-arm. Ctrl/Alt chords are ignored (`ignore_ctrl_alt`), not a latch-off.

Green / gray dot on the chip strip shows armed vs disarmed.

## Accept

- **Keyboard** default `accept_via = "suffix"`: typed `hel`, chip `hello` → inject `lo` plus optional space. `backspace_replace` deletes the token then sends the full word.
- **TextInput** splices `token_range` with the candidate (mid-word replaces the whole word).
- Highlight does not inject. `enterOrAcceptSuggestion` (Edge): highlight set → accept; else Enter / submit. Triggers use `sendKeyUnderLeftStickOrAcceptSuggestion` / `Right` (WhileHeld): highlight set → accept once then ignore the hold; else type the key under that stick. Pads stay type-only.
- Bumpers: `cycleSuggestion` highlights slot 0 from none; `cycleSuggestionPrev` highlights the last slot. `preselect = "none"`.

## Backends

**Ngram** (default): prefix scan of the frequency dictionary, then stupid backoff over prev words, blended with user-cache counts. Unigrams alone are enough for prefix completion. Packed 21-bit ids in `bigrams.bin` / `trigrams.bin` if present.

**Dictionary**: sorted `word` or `word<TAB>count`; rank by frequency, then length. Used for tests, `completion_dev` without a model dir, and fallback.

**User cache**: postcard file next to config (`completion-cache.bin`). Decayed unigram/bigram of accepted / submitted words. Novel words complete without rewriting the mmap tables.

English unigrams: `data/completion/en/unigrams.tsv` (FrequencyWords / OpenSubtitles, MIT). Large `*.bin` n-gram tables are gitignored. Rebuild:

```text
cargo run --bin completion_build -- --unigrams data/completion/en/unigrams.tsv --corpus sentences.txt --out data/completion/en
```

## Headless

```text
cargo run --bin completion_dev -- --text "hel" --cursor 3 --backend dictionary
cargo run --bin completion_dev -- --eval src/completion/fixtures/eval.txt --backend ngram
```

`--eval` replays a corpus as keystrokes, accepts a chip only on an exact upcoming match, and prints KSR plus next-word top-1/top-3.

Tests: `cargo test --lib completion`.
