# Configuration (`config.rs`)

This document describes how KOSK loads, watches, and writes its TOML configuration. The types and functions live in `src/config.rs`. The checked-in example is `config.toml` at the repository root, with controller mappings in `mappings.toml`. Layout geometry is a separate file (see [keyboard-layout.md](keyboard-layout.md)).

## What problem does this solve?

Almost every tunable in the program — stick scaling, which controller to open, debounce intervals, window position, the path of the layout TOML — is meant to be edited without rebuilding. Config is therefore a process-wide singleton: `config::init` runs once at startup, `config::get` clones the current value, and a file watcher reloads it when you save. Modes register `on_changed` callbacks so they can rebuild bindings and layouts after a reload.

That singleton is also why saving has to be careful. `Config` holds the expanded binding map, and a tape overlay or a CLI flag can make the live value differ from the file. Save patches the existing TOML instead of replacing it.

## Command line

`config::init` uses clap. The first positional argument is an optional path to a user TOML file. When it is omitted, kosk uses `%LOCALAPPDATA%\kosk\config.toml`, creating that file with `config_version = 1` if it does not exist. A path on the command line is also applied on top of the built-in defaults, and a migration is not written back to that path.

Optional flags:

- `--replay FILE` plays that `.krec` tape instead of opening HID. It overrides `preferred_controller` and `[replay].file`.
- `--keys-log FILE` sends outgoing keys to a log instead of injecting them. `-` means stdout. This overrides the `[key_sink]` table. See [key-sink.md](key-sink.md).
- `--ignore-recorded-config` keeps the on-disk config when replaying a tape that embedded one. See [record-replay.md](record-replay.md).
- `--mcp-controller` opens an exclusive virtual controller instead of any HID device, for scripted or agent-driven input. The same mode can be enabled with the `KOSK_CONTROLLER_MCP` environment variable, which also accepts a `host:port` bind address (the default is `127.0.0.1:5720`). This mode cannot be combined with replay. See the virtual-controller page.
- `--at-mouse` places the overlay at the mouse cursor, ignoring config `window_pos`. The file is not rewritten unless the user later saves from Move Window.

Auxiliary binaries that need config without parsing those flags call `init_from_path`.

## The `Config` struct

The deserialized struct is the source of truth after a successful load. Fields that matter across the rest of the docs:

- **`layouts`** maps layout names to file paths. A layout named `main` is required. `start_layout` must name an entry in that map.
- **`preferred_controller`** is an ordered list of families (`sc2`, `ps4`, `replay`). Omitted families are appended in built-in order. An empty list means Steam Controller 2, then DualShock 4.
- **`transparent`**, **`window_pos`**, **`scale_x` / `scale_y`** control the overlay. Position values are documented in [window-position.md](window-position.md). When `transparent` is true, **`keyboard_opacity`** (default `0.3`) sets clear/panel alpha on Keyboard and TextInput, and **`ui_opacity`** (default `0.92`) on Settings/Mappings/SelectKey/SelectLayout; both are ignored when `transparent = false`. Move Window is a see-through ghost and does not use `keyboard_opacity`.
- **`event_debounce_ms`** and **`event_debounce_repeat_ms`** are consumed by the event queue ([event-debounce.md](event-debounce.md)).
- **`controller_map`** is either an inline table or a string path to another TOML file. The checked-in config uses `controller_map = "mappings.toml"`.
- **`[sc2]`** and **`[ps4]`** describe device feel, not bindings: trigger thresholds, pad-origin mapping, and haptics. Button-to-action mapping is `controller_map`; these tables only change how the hardware feels under your thumbs.

- Aim profiles live nested under those tables: **`[sc2.pad]`**, **`[sc2.stick]`**, and **`[ps4.stick]`**. There is no `ps4.pad`, because the DualShock 4 has no pads. A touched Steam Controller pad uses the pad profile, while a physical stick uses that controller's stick profile. Replay replays already-mapped coordinates, so it uses the aim family stored on the tape (or Steam Controller 2 when the tape names none) without mapping twice.

- Each aim profile holds the same four knobs:

| Knob | Meaning |
|------|---------|
| `scale_x` / `scale_y` | How far a full deflection reaches on the keyboard |
| `warp` | Circle-to-square shaping: `0` leaves the stick circle alone, `1` fills the keyboard corners |
| `select_sticky` | Extra hit-test margin keeping the current key selected; `1` turns the margin off |
| `select_lock_ms` | How long the highlight freezes after a letter is sent, so a post-press twitch does not slide onto a neighbor |

- The settings screen edits these profiles through per-device entries (`Steam Controller Pads/Stick/Device`, `DualShock 4 Stick/Device`), and each value page writes its named device profile. See [keyboard.md](keyboard.md), [keyboard-layout.md](keyboard-layout.md), [menu.md](menu.md), and [controller.md](controller.md).
- **`record_file`** is a path template containing exactly one `%`, which becomes a three-digit index when recording starts.
- **`[replay]`** supplies a default tape path when `preferred_controller` starts with `replay` and `--replay` was not passed.
- **`[key_sink]`** chooses Enigo injection or a log file.
- **`[debug]`**, when present at all, enables debug overlays. Individual flags inside it turn on stick cursors, hitboxes, or stick bounds.
- **`[text_input]`** styles the single-line field in text-input mode.
- **`[completion]`** prediction backends, chip UI, typed-log latch, ngram weights, typo knobs, user cache. Type lives in `src/completion/settings.rs`. Relative model paths resolve against the config directory. See [completion.md](completion.md). Next-word pair-count setup is in the [README](../README.md#completion-next-word-setup).

The checked-in `config.toml`, `mappings.toml`, `old_sc.toml`, and `old_sc_symbols.toml` are built into the binary. The user file stores only values that differ from those defaults. Tables merge key by key. A scalar or array in the user file replaces the default. `config_version` is the schema this user file was written for. A missing value counts as 0. On load, ordered migrations bring it up to the version this binary understands, and the result is written back only for the implicit user path. A newer `config_version` refuses to start. Recorded config in a tape runs the same migrations before it is merged.

Saving writes a changed value into the user file and deletes a key whose value again matches the default. Bindings work the same way in the user `mappings.toml`: a changed binding is written, a removed default binding is `"none"`, and a binding that matches the default is removed. `"none"` is not an action.

Layouts, the word list, and `model_dir` are resolved beside the active config file (the user folder, or the folder of a config path given on the command line), then from the built-in copy for the default layouts and the English word list. There is no working-directory fallback. Recordings, replay tapes, and `completion-cache.bin` stay beside that same config file. A path on the command line is not written: saves change the running process only.

Relative paths (layouts, mappings file, record template, replay file, keys log) are resolved against the directory that contains the main config file, with the lookup above for layouts and completion data.

## `controller_map` as a file

Serde sees `controller_map` as either a nested table or a string. A string is opened relative to the config directory, parsed, and stored on `Config` as the expanded map. The path string is remembered beside the struct, from the raw file, so a later save can keep `controller_map = "mappings.toml"` and write binding changes into that sidecar. An inline table leaves the remembered path empty, and save patches the tables inside `config.toml`.

`mappings.toml` is grouped by mode (`[Keyboard]`, `[Settings]`, `[TextInput]`, `[MoveWindow]`, `[SelectLayout]`). Keys are binding specs such as `"triggerLeft"` or `"share + faceTop"`. Values are action names such as `"sendKeyUnderLeftStick"` or `"switchState.settings"`. Parsing of those strings is owned by each mode’s action enum; config only stores the text. See [bindings.md](bindings.md).

## Live reload

After the first successful load, `init_from_path` starts a `notify` watcher on the user config directory. Replay mode skips the watcher, because playback should not pick up live edits.

Only the files that make up the config trigger a reload: the config file, its `controller_map` file, and each layout file, whether it sits in that directory or elsewhere. Other files in the directory, such as `completion-cache.bin`, which is rewritten every time a suggestion is accepted, are ignored. When one of the config files is modified or created, `load_config` runs again. Reloads within the same second are ignored (“Reload debounced”) so editors that write in two steps do not apply a half-written file.

`load_config` validates that `main` exists, that `start_layout` names a real layout, that every layout can be found beside the active config file or as a built-in copy, and that `record_file` contains exactly one `%`. A parse or validation error prints to stderr and leaves the previous in-memory config in place.

Modules register with `config::on_changed`. Keyboard, settings, move-window, and text-input each reload their bindings (and the keyboard reloads layouts). The UI thread registers a callback that only calls `request_repaint`. An edit to `config.toml`, `mappings.toml`, or a layout file reloads. The mappings screen calls `config::notify_changed` after it saves.

## Saving

`config::save` compares the new `Config` with the built-in default and patches the user file with `toml_edit`. A value that differs is written. A value that matches the default is removed. `AppState` calls it when the user confirms a new overlay position in Move Window (`SaveWindowPos`). The mappings editor calls it after replacing `controller_map`, which updates the user `mappings.toml` the same way (`"none"` drops a default binding). Analog motion in that mode updates the live position only and does not save.

When a recording’s config overlay is active, `save` copies `window_pos` onto the remembered disk config and writes that, so a replay does not persist tape-only values. The path string is still kept.

## Recorded config

Tapes can embed a stripped copy of config (see [record-replay.md](record-replay.md)). `tape_config_toml` serializes the live `Config` and removes keys that should stay under the operator’s control: `layouts`, `record_file`, `replay`, `preferred_controller`, `key_sink`, `debug`, `transparent`, `keyboard_opacity`, `ui_opacity`, `window_pos`, `text_input`, and `completion`. On replay, `overlay_tape_config` migrates the blob, then merges it onto the on-disk config, again ignoring those keys if they appear in the blob. `--ignore-recorded-config` skips the merge.

`DISK_CONFIG` remembers the last file-backed snapshot so a save during overlay can write the disk view rather than the merged view.

## What this does not cover

**Action semantics are not in this module.** Config stores `"toggleShift"` as a string. `KeyboardAction::try_from` decides what that means.

**Device HID is not configured here beyond thresholds, pad origin, and aim profiles.** Button-to-action mapping is `controller_map`. Pad origin, trigger dead zones, and aim live on `[sc2]` / `[ps4]`.

## Summary

`config.rs` owns the process-wide TOML singleton, the CLI flags that override pieces of it, a file watcher for the main file, the mappings sidecar, and layout files, and a save path that patches the original TOML. A `controller_map` path string is remembered across load and save, and binding edits go to that sidecar; the sidecar is part of the watched set for external edits, and in-app saves additionally call `notify_changed` so the new bindings apply without waiting for the watcher round-trip. Device feel and binding tables are separate on purpose: thresholds belong next to the controller family, action names belong next to the mode that interprets them.
