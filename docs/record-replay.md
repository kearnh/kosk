# Recording and replay (`record.rs`, `replay.rs`)

This document describes KOSK’s input tapes: how a session is captured in `src/controller/record.rs`, how `src/controller/replay.rs` plays one back as a virtual controller, and how that interacts with config and the keys log. Debounce log lines that appear in a tape are explained in [event-debounce.md](event-debounce.md); this page is the file format and the two devices that write and read it.

## What problem does this solve?

Controller bugs that depend on timing — a doubled letter, a Shift that flickers, a pad-origin glitch — are hard to inspect live. A tape records timestamps, stick samples, buttons, layout switches, debounce accept/drop decisions, and the completion chips that were shown. You can replay the same HID-equivalent stream, optionally with the same debounce settings that were captured, and compare a keys log against what you intended to type. Playback of a current tape shows those chips again instead of asking the live dictionary.

Replay is also a `ControllerInput`. The rest of the app does not have a special “we are playing a file” branch in the keyboard. The controller thread opens `ReplayDevice` the same way it would open DualShock 4.

## Starting and stopping a recording

`toggleRecord` is a keyboard action that enqueues `Event::ToggleRecord`. `AppState` handles that event by calling `toggle_recording`. Replay ignores the toggle. If a recording is already in progress, it stops. Otherwise `record_file` from config must be set; the template is resolved against the config directory, the next free three-digit path is chosen, a `TapeHeader` is taken from the current keyboard, and `session().start` begins writing.

The template must contain exactly one `%`. `captures/kosk-%.krec` produces `captures/kosk-000.krec`, then `001`, and so on up to `999`.

The session is a process-wide object. The controller thread calls `tap_input` on every poll. Keyboard layout switches call `tap_layout`. The event queue logs debounce lines into the same session when a recording is active. Each time the completion session applies a new chip list, the UI thread calls `tap_suggestions` with the text before the cursor and the chips that were shown, including the current-word chip. Writes go to a background thread so a slow disk does not stall HID.

## Tape header

A file begins with the magic line `KOSKREC 1`. Version 2 (current) then includes:

- `version 2`
- `current_layout` and the name of the layout that was active when recording started
- `scale` with four floats: `scale_x`, `scale_y`, `stick_scale_x`, `stick_scale_y`
- a length-prefixed `config` blob: live config with a blacklist of keys stripped (see [config.md](config.md))
- length-prefixed `layout` blobs, one per named layout, containing the TOML source

Version 1 tapes have the same header and no suggestion lines. Version 0 tapes (no `version` line, no config blob) still parse. On replay, version 0 and 1 keep the process’s on-disk config, install layouts from the header, and keep the live completion engine. A version 2 tape does not store the dictionary or the user cache. `[completion]` stays off the config blob.

The header exists so replay can reconstruct the keyboard the user saw: key sizes, stick scales, debounce milliseconds, and which layout was current. Window position, transparency, preferred controller, and the keys sink are intentionally not taken from the tape; those are operator settings.

`AppState::apply_replay_header` installs the overlay config (unless `--ignore-recorded-config`) and asks the keyboard to `install_recorded_layouts`.

## Event stream

After the header, each line is a `RecordEvent` with a timestamp in microseconds from recording start:

- **Snapshot** — warped (and, for Steam Controller 2, origin-stretched) stick pair, a button bitfield in a fixed `BUTTON_ORDER`, and optional trigger values. This is already `ControllerInput::left_stick()` output, not raw HID.
- **Idle** — a poll where the device was not engaged.
- **Layout** — the user switched named layouts mid-tape.
- **Debounce** — `accept` or `drop` for a source, with elapsed time and whether hold-repeat was armed.
- **Suggestions** — the chip list just applied for the text before the cursor. The prefix and each chip are length-prefixed (`N:` then that many bytes) so spaces are safe. A chip written as `+N:text` is the current-word chip. No chips after the prefix means the strip was empty. The same prefix can appear more than once; playback hands those lists out in order.

Snapshots are the replay controller. Debounce and suggestion lines are not controller frames. Debounce lines are diagnostics; the player skips them when producing `ControllerInput` values, because the live `EventQueue` will debounce again. That is why captured config includes `event_debounce_ms`: replay should make the same accept/drop decisions if the code is unchanged. Suggestion lines are the chip list. The player does not recompute them.

## Replay device

`ReplayDevice::open` reads `config::replay_tape_path()` (`--replay` wins over `[replay].file`). It parses the tape, sets a process-wide `playback_origin` instant, and yields snapshots in real time by sleeping until each event’s `t_us`.

A process is either a normal keyboard or a playback for its whole lifetime. When playback starts and the tape is version 2 or newer, the completion session is created with `RecordedSuggestions` (`src/completion/recorded.rs`) instead of the ngram or dictionary engine. `suggest` returns the next recorded list for that prefix, or an empty list. It reports every token as a known word so the session does not add a second current-word chip. It does not write the user cache. The strip is drawn even when `[completion].enabled` is off. Chip columns, font, and the initial highlight still come from the live completion settings; bumpers on the tape move the highlight after that. Version 0 and 1 tapes keep the live engine.

`ReplayInput` implements `ControllerInput` from an `InputSnapshot`. `left_stick` and `right_stick` return the stored coordinates **without** applying `warp` or pad-origin stretch. Applying those again would double the mapping.

Layout events during playback call `keyboard::set_current_layout`. When the tape ends, `find_device`’s iterator finishes and `main` closes the window.

The keys log, if enabled, uses `playback_origin` for timestamps so a line’s microsecond column lines up with the tape rather than with whenever the log sink was constructed. See [key-sink.md](key-sink.md).

## What this does not cover

**A tape is not a video of the overlay.** It will not catch a drawing bug that does not affect input. It will catch “this sequence of sticks and buttons produced three `l`s.”

**Replay does not inject the recorded keys log.** It re-runs the app. If you changed debounce code, replay of an old tape is a regression test, not a guaranteed duplicate of the original injects.

**Version 0 and 1 tapes have no suggestion lines.** Replay of those files asks the live completion engine, so the chips can differ from the original session.

**Highlight and strip geometry are not on the tape.** The selected chip starts from the live `preselect` setting, then follows recorded bumper presses. A different column count can clip chips the recording showed.

**`toggleRecord` during replay is ignored.** You cannot nest recordings.

## Summary

A `.krec` file is a header (layouts plus a stripped config) followed by a timestamped stream of stick/button snapshots, idle marks, layout changes, debounce traces, and the completion chips that were shown. Recording is a side session tapped from the HID thread, the event queue, and the completion session. Replay is a `ControllerInput` that sleeps to the original timeline and feeds those snapshots back into `AppState`, using stored stick values as already-mapped coordinates. A version 2 playback serves the recorded chips from a completion backend and does not load the dictionary or the user cache.
