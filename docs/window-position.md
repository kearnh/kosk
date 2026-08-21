# Window position (`window_pos.rs`, `move_window.rs`)

This document describes how the overlay is placed on screen. `src/state/window_pos.rs` stores and resolves positions. `src/state/move_window.rs` and `src/state/move_window_action.rs` are the on-screen “Move Window” mode. `AppState::get_position` applies this every frame and sends the result to eframe; see [overview.md](overview.md).

## What problem does this solve?

The keyboard should sit where you can see it without covering the caret you are typing into. Sometimes that means a corner of the monitor. Sometimes it means “next to the mouse, as it was when KOSK launched.” Corners must stay corners after a DPI change. Pointer placement must not chase the cursor while you move the mouse to click elsewhere.

## `WindowPos`

The saved value in config is one of:

- `top left`, `top right`, `bottom left`, `bottom right` — named corners.
- `mouse pointer` — place beside the cursor captured at launch (or first use).
- `[x, y]` — an absolute top-left in egui points.

`resolve_position` turns a variant into coordinates given the current window size and monitor size. Corners are recomputed every time, so a larger keyboard still sits in the same corner. Absolute coordinates are clamped to keep the window on the monitor; if clamping changes the pair, `AppState` writes the clamped `Absolute` back to config.

`MousePointer` is **not** resolved in `resolve_position` (the fallback coordinates there are unused). `AppState::get_position` handles it via `PointerSnapshot`.

## Pointer snapshot

On Windows, `capture_pointer_snapshot` reads `GetCursorPos` and the work area of the monitor that contains the cursor (excluding the taskbar). Those values are stored in physical pixels. Later conversions divide by `pixels_per_point` so placement stays in egui points.

The snapshot is taken once, when `window_pos` is `mouse pointer` and no snapshot exists yet. After that, moving the real mouse does not move the overlay. That is deliberate: the window should not follow the pointer while you aim at a key.

Default placement is horizontal **right** of the cursor if the work area has room (`POINTER_GAP` is 12 px-equivalent), otherwise left. Vertically the top of the window **aligns** with the cursor so the overlay hangs down (bottom-right of the pointer), unless there is not enough work area below, in which case it hangs up. `place_near_pointer` applies those offsets independently and then clamps so the full window stays inside the work area.

Non-Windows builds return no snapshot; pointer placement cannot work there yet.

## Flips and rotate

Pointer placement is one of four slots around the captured cursor: bottom-right, bottom-left, top-left, and top-right. Each slot puts one corner of the window next to the cursor. After that, the same clamp as default placement keeps every edge on screen, so a slot near a monitor edge may slide rather than hang off.

- `FlipWindowLeftRight` mirrors left ↔ right and keeps top/bottom.
- `FlipWindowAboveBelow` mirrors hang-down ↔ hang-up and keeps left/right.
- `RotateWindow` walks the four slots counterclockwise: bottom-right → bottom-left → top-left → top-right → bottom-right. If launch started on another slot because of fit, rotate begins at that slot in the same ring. The fourth rotate returns to the start.

These events only run when `WindowPos` is `MousePointer`. On a corner or absolute position they are no-ops. Keyboard mappings in the checked-in file bind L4/R4 to the dedicated flips; `rotateWindow` exists as an action for a single-button cycle.

## Move-window mode

`StateId::MoveWindow` draws a 3×3 pad of corner snaps and nudges, plus Back to the menu. Mouse clicks set a `WindowPos` that `AppState::draw_ui` applies immediately (the same clamp-and-save path as `Event::MoveWindow`).

Controller actions (`MoveWindowAction`) enqueue events instead:

- Snap to each named corner.
- Nudge by 100 points along an axis (`WhileHeld`, so debounce applies).
- The same flip/rotate events as the keyboard.
- `switchState.*` to leave the mode.

Nudges are `WindowPos::Absolute` from the current on-screen coordinates. If you started on a named corner, the first nudge converts you to absolute placement.

The menu’s “Move” button switches to this mode. Controller navigation of the move-window grid is still unfinished relative to `todo.md`; dpad nudges work, but there is no highlighted cell walking the 3×3 like the menu’s selected index.

## What this does not cover

**The overlay never moves itself in response to the focused application’s caret.** Placement is config, the launch-time pointer, or this mode.

**Flips do not persist as `window_pos` strings.** The snapshot’s slot lives in memory. Restarting with `mouse pointer` recaptures the cursor and picks the default bottom-right (or left/top if that is what fits) again.

## Summary

Window position is a small enum in config plus, for pointer mode, a one-shot snapshot of cursor and work area. Corners and absolute coordinates are resolved against the current monitor. Pointer mode can flip or rotate around that frozen cursor. Move-window mode is the interactive editor for the same values, persisted through `config::save`.
