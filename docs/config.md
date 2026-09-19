# Configuration (`config.rs`)

This document describes how KOSK loads, watches, and writes its TOML configuration. The types and functions live in `src/config.rs`. The checked-in example is `config.toml` at the repository root, with controller mappings in `mappings.toml`. Layout geometry is a separate file (see [keyboard-layout.md](keyboard-layout.md)).

## What problem does this solve?

Almost every tunable in the program — stick scaling, which controller to open, debounce intervals, window position, the path of the layout TOML — is meant to be edited without rebuilding. Config is therefore a process-wide singleton: `config::init` runs once at startup, `config::get` clones the current value, and a file watcher reloads it when you save. Modes register `on_changed` callbacks so they can rebuild bindings and layouts after a reload.

That singleton is also why saving has to be careful. The in-memory `Config` is not always a faithful round-trip of the file on disk.

## Command line

`config::init` uses clap. The first positional argument is the path to the main TOML file. It is required; there is no implicit `config.toml` if you omit it.

Optional flags:

- `--replay FILE` plays that `.krec` tape instead of opening HID. It overrides `preferred_controller` and `[replay].file`.
- `--keys-log FILE` sends outgoing keys to a log instead of injecting them. `-` means stdout. This overrides the `[key_sink]` table. See [key-sink.md](key-sink.md).
- `--ignore-recorded-config` keeps the on-disk config when replaying a tape that embedded one. See [record-replay.md](record-replay.md).

Auxiliary binaries that need config without parsing those flags call `init_from_path`.

## The `Config` struct

The deserialized struct is the source of truth after a successful load. Fields that matter across the rest of the docs:

- **`layouts`** maps layout names to file paths. A layout named `main` is required. `start_layout` must name an entry in that map.
- **`stick_scale_x` / `stick_scale_y`** multiply analog deflection after it is mapped onto the keyboard (see [keyboard-layout.md](keyboard-layout.md)).
- **`stick_warp`** is the circle-to-square warp applied in `ControllerInput::left_stick` / `right_stick` (see [controller.md](controller.md)).
- **`preferred_controller`** is an ordered list of families (`sc2`, `ps4`, `replay`). Omitted families are appended in built-in order. An empty list means Steam Controller 2, then DualShock 4.
- **`transparent`**, **`window_pos`**, **`scale_x` / `scale_y`** control the overlay. Position values are documented in [window-position.md](window-position.md). When `transparent` is true, **`keyboard_opacity`** (default `0.3`) sets clear/panel alpha on Keyboard/TextInput and **`ui_opacity`** (default `0.92`) on Menu/Mappings/MoveWindow/SelectKey; both are ignored when `transparent = false`.
- **`event_debounce_ms`** and **`event_debounce_repeat_ms`** are consumed by the event queue ([event-debounce.md](event-debounce.md)).
- **`stick_select_lock_ms`** holds stick highlighting still after a letter is sent ([keyboard.md](keyboard.md)).
- **`stick_select_sticky`** is the extra hit-test margin for the key a stick is already on (`1` is off; default `1.25`). See [keyboard.md](keyboard.md).
- **`controller_map`** is either an inline table or a string path to another TOML file. The checked-in config uses `controller_map = "mappings.toml"`.
- **`[sc2]`** and **`[ps4]`** are device feel: trigger thresholds, pad-origin relative/stretch, haptics. They are not binding names.
- **`record_file`** is a path template containing exactly one `%`, which becomes a three-digit index when recording starts.
- **`[replay]`** supplies a default tape path when `preferred_controller` starts with `replay` and `--replay` was not passed.
- **`[key_sink]`** chooses Enigo injection or a log file.
- **`[debug]`**, when present at all, enables debug overlays. Individual flags inside it turn on stick cursors, hitboxes, or stick bounds.
- **`[text_input]`** styles the single-line field in text-input mode.
- **`[completion]`** prediction backends, chip UI, typed-log latch, ngram weights, typo knobs, user cache. Type lives in `src/completion/settings.rs`. Relative model paths resolve against the config directory. See [completion.md](completion.md). Next-word pair-count setup is in the [README](../README.md#completion-next-word-setup).

Relative paths (layouts, mappings file, record template, replay file, keys log) are resolved against the directory that contains the main config file.

## `controller_map` as a file

Serde sees `controller_map` as either a nested table or a string. A string is opened, parsed as TOML whose root is `HashMap<StateId, HashMap<ControllerBinding, String>>`, and stored in memory as that map. The original path is not kept on `Config`.

That is convenient for loading. It is the cause of the save bug described below.

`mappings.toml` is grouped by mode (`[Keyboard]`, `[Menu]`, `[TextInput]`, `[MoveWindow]`). Keys are binding specs such as `"triggerLeft"` or `"options + faceTop"`. Values are action names such as `"sendKeyUnderLeftStick"` or `"switchState.menu"`. Parsing of those strings is owned by each mode’s action enum; config only stores the text. See [bindings.md](bindings.md).

## Live reload

After the first successful load, `init_from_path` starts a `notify` watcher on the config file and on every layout file. Replay mode skips the watcher, because playback should not pick up live edits.

When a watched file is modified, `load_config` runs again. Reloads within the same second are ignored (“Reload debounced”) so editors that write in two steps do not apply a half-written file. If the set of layout paths changes, the watcher thread starts a replacement watcher and exits.

`load_config` validates that `main` exists, that `start_layout` names a real layout, that every layout file exists on disk, and that `record_file` contains exactly one `%`. A parse or validation error prints to stderr and leaves the previous in-memory config in place.

Modules register with `config::on_changed`. Keyboard, menu, move-window, and text-input each reload their bindings (and the keyboard reloads layouts). The UI thread registers a callback that only calls `request_repaint`. The watcher does **not** watch `mappings.toml`. If you edit mappings while the process is running, those edits are not picked up until something else reloads config (for example saving `config.toml`, or restarting). Combined with the save bug, that is easy to trip over.

**Footnote:** Not watching `mappings.toml` is a bug. See [todo.md](../todo.md).

## Saving

`config::save` writes `toml::to_string_pretty` of a `Config` back to the original path. `AppState` calls it when the user moves the window, so that `window_pos` persists.

Because `controller_map` in memory is the expanded HashMap, a full save serializes it as inline `[controller_map.Keyboard]` tables and drops the `"mappings.toml"` path. After that, editing `mappings.toml` has no effect: the next load uses the inlined copy inside `config.toml`. The same dump can also reorder or rewrite other tables.

When a recording’s config overlay is active, `save` is more conservative: it copies `window_pos` onto the remembered disk config and writes that, so a replay does not persist tape-only values. That path still does not restore a `controller_map` file reference if the disk file had already been rewritten earlier.

Until that is fixed, treat `config::save` as “persist window position, possibly at the cost of rewriting the rest of the file.” Prefer editing `config.toml` and `mappings.toml` by hand and restarting if mappings stop applying.

**Footnote:** Inlining the mappings table on save (and forgetting the `mappings.toml` path) is a bug. See [todo.md](../todo.md).

## Recorded config

Tapes can embed a stripped copy of config (see [record-replay.md](record-replay.md)). `tape_config_toml` serializes the live `Config` and removes keys that should stay under the operator’s control: `layouts`, `record_file`, `replay`, `preferred_controller`, `key_sink`, `debug`, `transparent`, `keyboard_opacity`, `ui_opacity`, `window_pos`, `text_input`, and `completion`. On replay, `overlay_tape_config` merges the blob onto the on-disk config, again ignoring those keys if they appear in the blob. `--ignore-recorded-config` skips the merge.

`DISK_CONFIG` remembers the last file-backed snapshot so a save during overlay can write the disk view rather than the merged view.

## What this does not cover

**Action semantics are not in this module.** Config stores `"toggleShift"` as a string. `KeyboardAction::try_from` decides what that means.

**Device HID is not configured here beyond thresholds and pad origin.** Button-to-action mapping is `controller_map`. Pad origin and trigger dead zones are `[sc2]` / `[ps4]`.

## Summary

`config.rs` owns the process-wide TOML singleton, the CLI flags that override pieces of it, a file watcher for the main file and layout files, and a save path that currently round-trips the in-memory struct rather than the original file. Bindings may live in a sidecar `mappings.toml`, but that path is forgotten after deserialize, and the watcher does not include that sidecar. Device feel and binding tables are separate on purpose: thresholds belong next to the controller family, action names belong next to the mode that interprets them.
