# Code explanations

These pages describe how KOSK works as it is written today. Each document covers one subsystem: why that code exists, which types carry the work, and how a request travels through the process. They are meant to be read in full sentences, without assuming you were present for earlier design discussions.

Start with [overview.md](overview.md) if you are new to the repository. After that, follow the links from each page, or use the list below.

## Reading order

1. [overview.md](overview.md) — process, window, and the two threads.
2. [config.md](config.md) — `config.toml`, CLI flags, live reload, and saving.
3. [controller.md](controller.md) — the `ControllerInput` trait, button names, discovery, and stick warp.
4. [devices.md](devices.md) — DualShock 4 and Steam Controller 2 HID, including pad-origin stretch.
5. [bindings.md](bindings.md) — how mappings.toml turns button holds into typed actions.
6. [record-replay.md](record-replay.md) — `.krec` tapes, `toggleRecord`, and the replay controller.
7. [app-state.md](app-state.md) — `AppState`, modes, and `process_events`.
8. [event-debounce.md](event-debounce.md) — the event queue that throttles held buttons.
9. [key-sink.md](key-sink.md) — injecting keys with Enigo, or logging them instead.
10. [window-position.md](window-position.md) — overlay placement, pointer snapshot, flips, and the move-window mode.
11. [keyboard.md](keyboard.md) — stick highlighting, sending keys, and sticky modifiers.
12. [keyboard-layout.md](keyboard-layout.md) — layout TOML, hitboxes, and `when` display clauses.
13. [menu.md](menu.md) — the current two-item menu.
14. [text-input.md](text-input.md) — the single-line field that intercepts typing.
15. [completion.md](completion.md) — the prefix dictionary engine (not wired into the keyboard UI yet).

## Plans

Working notes that describe intended work, not shipped behavior, live under [plans/](plans/). They may be terse. Do not treat them as a description of the running program.

The code-explanation series does not include `todo.md` at the repository root. That file is a personal checklist.
