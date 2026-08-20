# Text completion (`completion/`)

This document describes the completion library as it exists in `src/completion/`. The module can suggest words from a sorted dictionary given the text around a caret. Nothing in the on-screen keyboard or in [text-input mode](text-input.md) calls it yet. Intended UI (suggestion chips, controller accept/cancel) is sketched in [plans/completion.md](plans/completion.md), not here.

## What this module is

`kosk::completion` is a synchronous, in-process prefix matcher. You give it the string before the caret, the string after the caret, and a maximum number of results. It returns whole-word `Suggestion` values whose text is meant to replace the token currently being typed.

The public surface is small:

- `CompletionEngine` — a trait with `suggest(&CompletionContext) -> Vec<Suggestion>`.
- `DictionaryEngine` — the only implementation, backed by a `Vec<String>`.
- `CompletionContext` — `prefix`, `suffix`, `max_results`.
- `split_at_cursor` and `word_prefix_token` — helpers for turning a buffer plus byte index into that context.

`src/completion/mod.rs` re-exports those types. There is no UI, no threading, and no file format beyond “newline-separated words.”

## Splitting at the caret

`split_at_cursor(text, cursor_byte)` returns `(prefix, suffix)` if `cursor_byte` is on a UTF-8 character boundary and not past the end of the string. Splitting inside a multi-byte character returns `None`. That is the same convention text-input mode uses for `cursor_pos`.

`completion_dev` calls this first. A production UI would do the same from the field’s buffer and caret.

## The current token

`DictionaryEngine` does not search using the entire prefix. It calls `word_prefix_token(prefix)`, which walks backward from the end of `prefix` while characters are ASCII-style alphanumeric or `_`. If the last character is whitespace or punctuation, the token is empty.

So `"hello wor"` with the caret after `r` yields token `wor`. `"hello "` with the caret after the space yields `""`. Empty tokens produce no suggestions. There is no “frequent words when idle” list.

The suffix is stored on `CompletionContext` for future engines (closing a quote, morphology that cares about what follows). `DictionaryEngine::suggest` never reads it.

## `DictionaryEngine`

`from_wordlist_text` splits on lines, trims, drops empties, sorts, and deduplicates. `embedded_demo` loads `src/completion/test_words.txt` at compile time for tests and for `completion_dev` when no file is passed.

`suggest` finds the first word not lexicographically less than the token (`partition_point`), then scans forward while `starts_with(token)`, stopping at `max_results` (at least 1). Order is dictionary order, not frequency or keyboard distance. Matching is exact prefix, case-sensitive, with no edit-distance fallback.

That is the whole ranking model. Replacing it later is the reason the trait exists: a fuzzy engine can implement `CompletionEngine` without changing callers.

## The `completion_dev` binary

`src/bin/completion_dev.rs` is a clap tool that does not open a window or a controller. From the repository root:

```text
cargo run --bin completion_dev -- --text "hello wor" --cursor 9
```

`--cursor` is a byte index. `--max` defaults to 8. `--wordlist PATH` loads a UTF-8 file; otherwise the embedded demo list is used. Suggestions print as a numbered list, or `(no suggestions)`.

Unit tests live next to the library (`cargo test completion`). They cover prefix hits, empty tokens, the max cap, and cursor splitting including emoji boundaries.

## What this does not cover

**The engine is not consulted when you type in text-input mode.** Wiring would mean, on buffer change, building a `CompletionContext` and drawing the returned strings. That work is not in `text_input.rs`.

**There is no user wordlist in the config directory, and no network.** Privacy of the planned UI (offline lists only) is already true of this library because it never fetches.

**Typo correction, n-grams, and morphological completion are not implemented.** The plan file records why they were deferred.

## Summary

Completion is a pluggable `suggest` trait and one dictionary implementation that binary-searches a sorted word list for exact prefixes of the alphanumeric token at the caret. You can exercise it with `completion_dev`. The overlay does not show suggestions yet; when it does, this module is the piece that should stay synchronous and UI-agnostic, with the plan file describing the chips and controller actions around it.
