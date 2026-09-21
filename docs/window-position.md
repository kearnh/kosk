# Window position (`window_pos.rs`, `move_window.rs`)

This document describes how the overlay is placed on screen. `src/state/window_pos.rs` stores and resolves positions. `src/state/move_window.rs` and `src/state/move_window_action.rs` are the on-screen “Move Window” mode. Button glyphs used while placing live in `src/ui/controller_glyph.rs`. `AppState::get_position` applies this every frame and sends the result to eframe; see [overview.md](overview.md).

## What problem does this solve?

The keyboard should sit where you can see it without covering the caret you are typing into. Sometimes that means a corner of the monitor. Sometimes it means “next to the mouse, as it was when KOSK launched.” Corners must stay corners after a DPI change. Pointer placement must not chase the cursor while you move the mouse to click elsewhere.

## `WindowPos`

The saved value in config is one of:

- `top left`, `top right`, `bottom left`, `bottom right` — named corners.
- `mouse pointer` — place beside the cursor captured at launch (or first use).
- `[x, y]` — an absolute top-left in egui points.

`resolve_position` turns a variant into coordinates given the current window size and monitor size. Corners are recomputed every time, so a larger keyboard still sits in the same corner. Absolute coordinates are clamped to keep the window on the monitor; if clamping changes the pair, `AppState` writes the clamped `Absolute` back to config, except while Move Window is active so dragging cannot rewrite the file.

`MousePointer` is **not** resolved in `resolve_position` (the fallback coordinates there are unused). `AppState::get_position` handles it via `PointerSnapshot`.

`--at-mouse` forces `MousePointer` at process start and ignores config `window_pos`. That override is not written to disk unless the user later saves from Move Window.

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

These events only run when `WindowPos` is `MousePointer`. On a corner or absolute position they are no-ops. They are Keyboard actions, not Move Window actions.

## Move-window mode

`StateId::MoveWindow` turns the overlay into a layout-sized translucent rectangle. Analog sticks and pads move it as a cursor. Disk save happens only on confirm.

Entry (Settings **Move**) snapshots the current `WindowPos`. Analog does not go through the binding engine. Each HID tick:

- Right analog wins over left. A touching pad is input even at the pad center.
- Pads are mouse-relative: finger travel on the pad (raw samples, first contact is no jump) moves the overlay. A swipe across the full pad (`-1` → `1`) crosses one monitor. Samples are EMA-smoothed; window motion is whole pixels from a leftover remainder. Lifting the thumb drops motion from the last 40ms so peel-off does not nudge the ghost.
- Sticks stay velocity: deadzone `0.15` on the max axis, full deflection crosses the monitor in one second. Stick up decreases window `y`.
- The live position becomes `WindowPos::Absolute` and is not written to config.

`save` (shipped map: `faceBottom`) writes config: if analog never moved, the original variant is kept; otherwise the live `Absolute` is stored. Then the mode returns to settings. `switchState.settings` (shipped map: `faceRight`) restores the snapshot and leaves without saving. Both are Edge actions. The prompts show Steam Input knockout glyphs plus Save/Cancel as labels, not buttons.

## Button glyphs

`src/ui/controller_glyph.rs` maps every `ControllerButton` for Steam Controller 2 and DualShock 4 to an embedded knockout SVG from Steam’s `controller_base/images/api/knockout`. Replay and the virtual controller pick a family from `preferred_controller` (first non-replay name, else SC2). Later screens can call `controller_glyph::show` with a family and button.

## What this does not cover

**The overlay never moves itself in response to the focused application’s caret.** Placement is config, the launch-time pointer, `--at-mouse`, or this mode.

**Flips do not persist as `window_pos` strings.** The snapshot’s slot lives in memory. Restarting with `mouse pointer` recaptures the cursor and picks the default bottom-right (or left/top if that is what fits) again.

## Summary

Window position is a small enum in config plus, for pointer mode, a one-shot snapshot of cursor and work area. Corners and absolute coordinates are resolved against the current monitor. Pointer mode can flip or rotate around that frozen cursor. Move-window mode slides the overlay with analog input and writes config only when the user saves.
