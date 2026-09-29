# Key sink (`key_sink.rs`)

This document describes where accepted `SendKey` and `SendText` events go. The trait and implementations are in `src/state/key_sink.rs`. `build.rs` injects git metadata that file logs include as a header.

## What problem does this solve?

The overlay must type into other applications. egui widgets cannot do that. KOSK uses [Enigo](https://github.com/enigo-rs/enigo) to synthesize OS-level key events. When you are diagnosing debounce or replay, injecting into the focused app is the wrong tool: you want a timestamped log of what *would* have been typed. `KeySink` is that fork.

## The trait

`KeySink` has two methods: `key(key, direction)` for virtual keys (press, release, or click) and `text(str)` for Unicode strings. Both return `Result`. `AppState::process_events` prints errors and continues.

`open_key_sink` chooses an implementation at process start. It is not swapped later.

## Enigo

`EnigoSink` is the default (`[key_sink]` omitted or `type = "enigo"`). `key` and `text` are thin wrappers around Enigo’s keyboard API.

This is why [keyboard sending](keyboard.md) sometimes uses `SendText` and sometimes `SendKey(Unicode)`: Enigo’s text injection does not combine with held modifiers, and Enigo’s virtual-key path does not produce a capital letter without Shift. The sink itself does not know about sticky modifiers; it performs the events it is given.

## Log sink

`LogSink` writes one line per call:

```text
<microseconds> key Shift Press
<microseconds> key Unicode('a') Click
<microseconds> text hello
```

File logs (not stdout) start with a comment line `# git <commit>` or `# git <commit> dirty`. Those values come from `env!("GIT_COMMIT")` and `env!("GIT_DIRTY")`, which `build.rs` fills by running `git rev-parse` and `git status --porcelain` at compile time. A jujutsu checkout that still has a colocated `.git` will report the git commit; if git is unavailable the commit is `unknown` and dirty is treated as true.

Timestamps are microseconds from `LogSink` construction, **unless** a replay is in progress. `replay::playback_origin` is the wall clock when the tape started. Using that origin makes a keys log line up with `t_us` in the `.krec` file.

`--keys-log` on the command line wins over `[key_sink]`. `-` is stdout and skips the git header so the stream is only data lines. A relative path in config is resolved against the config directory; a CLI path is used as given.

## What this does not cover

**The sink does not debounce.** If two `SendText("l")` events reach it 55 ms apart, two `l`s are typed or logged. Filtering already happened in `EventQueue`.

**The sink does not know about layouts.** Layout files decide which character to send; this module only performs OS injection or I/O.

## Summary

Outgoing typing is a trait so tests and diagnostics can avoid moving the user’s cursor. Production uses Enigo. `--keys-log` or `[key_sink] type = "log"` writes a timestamped trace, with file logs tagged by the git commit baked in at build time, and with timestamps pinned to replay origin when a tape is playing.
