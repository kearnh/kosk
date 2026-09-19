# Keyboard layouts (`layout.rs`, `key.rs`, `when.rs`)

This document describes how a layout TOML file becomes an on-screen keyboard: key definitions, display rules, geometry, and stick hit-testing. The code lives in `src/state/keyboard/layout.rs`, `key.rs`, and `when.rs`. Checked-in layouts include `old_sc.toml` (`main`) and `old_sc_symbols.toml` (`symbols`) from `config.toml`. Keyboard mode’s use of the layout is in [keyboard.md](keyboard.md).

## What a layout file is

A layout is TOML with global padding and font size, optional stick bounds, and a list of rows. Each row has an indent, a height, and a list of keys. Distances in the file are **unscaled units**. At load time they are multiplied by `config.scale_x` and `config.scale_y` so one file can be enlarged without rewriting every width.

`KeyboardLayout::load_from_file` reads the file, keeps the original TOML string for recording headers, and fills scale fields from config (or from a tape’s captured scales during replay). Hitboxes are not computed at load. They wait until a draw captures each button’s center from egui, because the true pixel position depends on how egui laid out that frame. The first capture also translates stick bounds by the offset from the layout-local first key to that captured centre, so chrome that is already present (chip strip, window pad) does not leave the bounds sitting too high. If a later draw moves the keys, the same translation is applied again. A layout switch clears captured centres on the incoming board so the next draw is that first-capture path.

## Keys: `RawKey` and `Key<T>`

Each button has a `key` field that deserializes as `Key<RawKey>`: a `normal` value and an optional `shift` value. A bare string is normal-only; a table `{ normal = "1", shift = "!" }` is an explicit pair. If shift is omitted, `ToShifted` uppercases a single character and leaves other variants alone.

`RawKey` is what actually gets sent or executed:

- A single character becomes `Key(char)`.
- `skip` (any case) is a spacer: no widget, just width.
- A known `KeyboardAction` name becomes `Action(...)`, so a key can be `toggleShift` or `switchState.menu`.
- A name Enigo understands (`Backspace`, and so on) becomes `Enigo`.
- Anything else, including several characters, becomes `Text`.
- A leading `\` forces a literal `Text` (or, after the slash, the remainder as text) so that a label which would otherwise parse as an action can still be a character sequence. `\\` starts a literal that itself begins with a backslash.

Widths default to 1 unit. `selectable = false` draws the key but excludes it from stick hit-testing. `display_modifiers` is the hook keyboard mode uses to paint `ctrl+alt` on Space.

## Display and `when` clauses

What is printed on a key is independent of what it sends. `display` may be:

- omitted — a character key shows that character (shifted if the layer is on); other `RawKey`s show `?` unless you set display.
- a constant string.
- a shifted pair of strings.
- a list of **rules**. Each rule has `text`, optional `when`, and optional button/text colors (British spellings `button_colour` / `text_colour` are accepted). The first rule whose `when` is missing or true wins.

`when` strings parse in `src/when.rs` into a small boolean AST. Canonical flags are `modifier.shift` (alias `shift`), `modifier.ctrl` / `ctrl`, `modifier.alt` / `alt`, `modifier` (any of those three), `recording`, `replay`, `suggestionSelected` (a chip is highlighted), and `completionActive`. You can combine them with `&&`, `||`, `!`, and parentheses. `WhenContext` is filled each frame from keyboard state and the record/replay session. That is how a Rec/Stop key can change label while a tape is running without being a different `RawKey`. Layout `when` is display-only. Controller mappings use the same language to choose which action fires; see [bindings.md](bindings.md).

## Geometry and hitboxes

After egui draws, `update_geometry` stores centers and calls `calculate_hitboxes`. Selectable, non-skip keys get a circle or an ellipse centered on the button:

- Wide keys (width / row height ≥ 1.2) get an ellipse whose radii are half-width and half-height times √2, so the ellipse roughly covers the rectangle and a bit more.
- Narrower keys get a circle of radius `scale_x * 1.125`.

`selectable = false` and skip keys get no hitbox.

Stick rest positions come from optional top-level `stick_rest_left` / `stick_rest_right` fields: each is a `[row, column]` index into `rows` / `rows.keys` (0-based). The referenced key must exist and must not be `Skip` (Skip has no captured centre). If a field is omitted, that side’s rest centre is `(0, 0)`. The checked-in `main` layout points at home-row `d` / `k`; `symbols` uses the same row/column slots. A layout may use a different rest. After a switch, analog mapping is offset by the rest delta so the cursor does not jump; idle clears that offset.

`stick_to_cursor_left` / `_right` take a warped stick in −1…1, multiply by `scale_* * stick_scale_*`, and add the rest center. That point is then hit-tested.

## Stick bounds

Optional `stick_bounds.left` / `right` are lists of rectangles in unscaled units. They are scaled at load. When key centers later move, the same translation is applied so clamping stays on the keys. If they are present and the cursor is outside all of them, the cursor is clamped to the nearest point on the nearest rectangle before hit-testing. That keeps the left stick from highlighting keys on the right half of a split keyboard when you push to the edge.

`get_key_at` walks hitboxes and picks the containing shape with the smallest rim-fraction score: `0` at that key’s centre and `1` on its rim (`distance² / r²` for circles; the usual ellipse implicit value for wide keys). Overlapping hitboxes therefore resolve to the key whose centre you are closer to relative to that key’s own size, not to draw order, and not to raw pixel distance (which would let a wide ellipse beat a letter almost everywhere they overlap). Keyboard mode can pass the previous cell and `stick_select_sticky` into the same picker so that cell keeps winning until a neighbor is clearly closer; `get_key_at` itself does not apply that margin.

## Debug drawing

When `[debug]` is set, `draw_debug` can paint the warped stick positions (`show_stick_cursors`), the hitboxes (`show_hitboxes`), and the bound rectangles (`show_stick_bounds`). Cursor drawing reads the latest snapshot from `DebugPlugin`.

## What this does not cover

**This module does not enqueue events.** It answers “which `RawKey` is under this stick?” and “what should this button look like?”

**Config `scale_*` and `stick_scale_*` are applied here, but defined in config.** Replay can override them via the tape header without editing the layout file.

## Summary

A layout TOML is a scaled grid of keys, each of which names a `RawKey` (character, text, Enigo key, action, or skip) and optionally a richer display with `when` clauses. Pixel centers come from egui; hitboxes and optional bound rectangles then turn analog samples into the same keys the user sees, ranking overlaps by rim-fraction. Recordings store the TOML source and the scales so replay can rebuild that geometry.
