# Text completion (`completion/`)

Word and next-token prediction for Keyboard and TextInput. UI talks only to the session. Backends never see egui, HID, or `config::get()`.

## Layers

1. **Context** — `(text, cursor_byte)` → current token, `token_range`, previous words, case flags (`context.rs`). Keyboard uses a session-scoped log of keys kosk actually sent; TextInput uses the local buffer and caret.
2. **Session** — debounce, latest-wins worker slot, generation filter, highlight, armed latch (`session.rs`). `None` from a backend means abort (stale generation), not “no suggestions.”
3. **`CompletionBackend`** — `suggest(ctx, abort) -> Option<Vec<Candidate>>`. Factory: `backend_from_config`. Default `ngram`; `fallback = "dictionary"` if model files are missing.
4. **Chips** — reserved strip above the keys (`state/completion_ui.rs`). Cycle never injects. Accept injects.

Config lives in `[completion]` (`src/completion/settings.rs`). Relative paths resolve against the config directory. `completion` is in `TAPE_CONFIG_SKIP`.

## Armed latch (Keyboard)

Starts armed (`start_armed`). Arrow / Home / End / PageUp / PageDown / Insert / Delete and paste **disarm**: stop logging, hide chips, stop predicting. Stays off until `toggleCompletion`. Re-arm clears the log (`clear_log_on_arm`). Enter does not re-arm. Ctrl/Alt chords are ignored (`ignore_ctrl_alt`), not a latch-off.

Green / gray dot on the chip strip shows armed vs disarmed.

## Accept

- **Keyboard** default `accept_via = "suffix"`: typed `hel`, chip `hello` → inject `lo` plus optional space. `backspace_replace` deletes the token then sends the full word. If the chip is not a case-insensitive prefix of the token (a typo correction such as `thr` → `the`), accept always uses backspace-replace for that injection, even when the config is suffix.
- After accept, if `insert_space_on_accept` added a trailing space and the next typed character is in `[completion].eat_space_before`, that space is deleted so the symbol sits against the word (`hello/` not `hello /`). If the character is also in `[completion].space_after` (default `,.!?;:`), a space is inserted after the mark (`hello? `). The latch is one-shot: a letter, backspace, arrow, or cancel drops it. An empty `eat_space_before` turns eating off; an empty `space_after` still eats but never re-spaces. Keyboard overlay sends a Backspace into the focused app (and an extra space when re-spacing); TextInput edits the local buffer. Layout switch does not consume the latch, so accept then a symbols-board period still eats.
- **TextInput** splices `token_range` with the candidate (mid-word replaces the whole word).
- Chip `dim_typed_prefix` and `label = remainder` apply only when the token is a prefix of the chip. A correction paints the full word.
- Highlight does not inject. Bumpers cycle chips. `faceBottom` is `acceptSuggestion` when `suggestionSelected`, else Enter / TextInput `submit`. Triggers are `acceptSuggestion` when `suggestionSelected`, else type under that stick. Pads stay type-only. `faceRight` is `cancelSuggestion` only while a chip is highlighted. The binding engine latches the chosen action until release so accept does not turn into a typed letter on the same hold.
- Bumpers: `cycleSuggestion` highlights slot 0 from none; `cycleSuggestionPrev` highlights the last slot. `preselect = "none"`. Typing keeps the highlight if that chip is still in the new list (even in another column); otherwise `reset_highlight_on_refresh` clears it.
- The engine returns at most `max_suggestions`. The strip shows at most `columns * rows` and drops the rest. Raise both to show a reserved full-word correction plus prefix completions. `reserve_slots` sizes the strip to that grid even when empty so key centres do not jump.

## Backends

**Ngram** (default): prefix scan of the frequency dictionary, then stupid backoff over prev words, blended with user-cache counts. Unigrams alone are enough for prefix completion. Packed 21-bit ids in `bigrams.bin` / `trigrams.bin` if present. Empty pair tables still load; next-word then falls back to top unigrams (`you` / `i` / `the`) and the process prints a warning. User-cache continuations of the previous word are merged into next-word candidates before that fallback.

**Dictionary**: sorted `word` or `word<TAB>count`; rank by frequency, then length. Used for tests, `completion_dev` without a model dir, and fallback. With `typo_tolerance`, a distance-1 fuzzy prefix scan runs after the exact prefix range: neighbor substitution (layout map injected by the keyboard), adjacent transposition (length ≥ 2), omitted key, extra key. Identity (`the` after typing `the`) is never chipped; longer prefixes (`there`) still are. One slot is reserved for the best full-word correction (`thr` → `the`); remaining slots are exact prefixes, then other fuzzy hits.

**User cache**: postcard file next to config (`completion-cache.bin`). Decayed unigram/bigram of accepted / submitted words. Novel words complete without rewriting the mmap tables. Learned pairs also generate next-word chips when tables are empty.

Neighbor keys are precomputed from letter-key centres when keyboard geometry updates, filtered to keys reachable from the same stick bounds. The last letter-layout map is kept when the current board has fewer than ten letters (symbols layout). Backends see only `HashMap<char, Vec<char>>` on the context; they do not call layout code.

English unigrams: `data/completion/en/unigrams.tsv` (FrequencyWords / OpenSubtitles, MIT). Source wordlist for prefix completion, not a pack output. `completion_build` writes gitignored `vocab.txt` and `*.bin` only. Pair counts are required for context next-word. Unigrams-only pack is not enough:

```text
cargo run --bin completion_build -- --unigrams data/completion/en/unigrams.tsv --out data/completion/en
```

That command writes empty `bigrams.bin` and still succeeds. Download [count_2w.txt](https://norvig.com/ngrams/count_2w.txt) and pack with `--bigrams`, then confirm the printed bigram count is not zero. Full commands and a `completion_dev` check are in the [README](../README.md#next-word-after-a-space).

## Headless

```text
cargo run --bin completion_dev -- --text "hel" --cursor 3 --backend dictionary
cargo run --bin completion_dev -- --eval src/completion/fixtures/eval.txt --backend ngram
```

`--eval` replays a corpus as keystrokes, accepts a chip only on an exact upcoming match, and prints KSR plus next-word top-1/top-3.

Tests: `cargo test --lib completion`.
