# On-screen keyboard (`keyboard/mod.rs`, `keyboard_action.rs`)

This document describes the keyboard mode: highlighting keys under the sticks, turning controller actions into events, and sticky modifiers. The types live in `src/state/keyboard/mod.rs` and `src/state/keyboard/keyboard_action.rs`. How a layout file becomes hitboxes is [keyboard-layout.md](keyboard-layout.md). How held triggers are throttled is [event-debounce.md](event-debounce.md).

## What this mode is

Keyboard mode is the default `StateId`. It draws the current layout as a grid of egui buttons, colors the keys nearest the left and right analog samples, and sends the highlighted key when a binding such as `sendKeyUnderLeftStick` fires. Mouse users can click a button instead; that path uses `EventSource::MouseClick` and the same `send_key` function.

`KeyboardState` is a process-wide singleton. `init` constructs it from config; `on_changed` reloads layouts and bindings. `AppState` talks to it through `keyboard::with_mut`.

## Stick selection

Each poll, `handle_controller_input` asks the current `KeyboardLayout` which selectable **cell** (row and column) is under the left stick and which is under the right stick. The glyph that cell sends is looked up later with `shift_state`, when the key is drawn or typed.

Among hitboxes that contain the cursor, the layout ranks keys by how far the cursor is from that key’s centre toward that key’s own rim (`0` at the centre, `1` on the rim). That unit is the same for letter circles and wide-key ellipses. The nearest rim-fraction wins, unless this stick already had a cell: that cell stays selected while it still contains the cursor and its score is at most `stick_select_sticky` times the next-best score (`1` turns the margin off; the checked-in default is `1.25`). A committed move onto a neighbor, or leaving the old hitbox, switches immediately. Idle rest highlight uses the unbiased pick at stick `(0, 0)`, not the sticky cell.

Those answers are not always applied immediately. After a stick-send action succeeds, that stick’s selection is frozen for `stick_select_lock_ms` (100 ms in the checked-in config). The lock exists so a small pad twitch right after a press does not slide onto a neighbor and type a different letter while the trigger is still down. When the lock expires, highlighting follows the stick again. The sticky margin is what stops a twitch from changing the key *before* send; the lock still runs after send.

A layout switch recaptures geometry on the next draw as a cold start (window size, key centres, stick bounds). It does not copy those from the previous layout. Selection is remapped by the same screen pixels onto the new board, so different row heights and key widths still land on whatever key is under the cursor. If that pixel is empty, that stick has no selection. Analog rest centres may differ; the mapping adds `origin1 − origin2` so the cursor does not jump. That bias clears when the device goes idle, so the next rest uses the new origin. Until the stick or pad moves by more than a small deadzone, selection also stays on the remapped cell.

If the selected **cell** on a side **does** change (lock not holding, or lock expired), the keyboard calls `events.clear_toggle_suppress` for that side’s usual sources (`triggerLeft` and `padLeft`, or the right pair). That is how you can hold a trigger, toggle Shift, then slide onto Ctrl and toggle Ctrl without releasing. Details are in the event-queue doc. Toggling Shift on the same cell is not a selection change.

When the device yields `None` (idle), selection is cleared, the binding engine is reset, and layout-switch send suppression is cleared.

## `KeyboardAction`

Mappings and on-layout keys both parse through `KeyboardAction::try_from`. Unit names are case-insensitive (`toggleShift`, `ToggleShift`). Parameterized names use a dot: `switchState.menu`, `switchLayout.symbols`, `sendKey.c`.

`sendKey.space`, `sendKey.enter`, `sendKey.tab` become character sends. `sendKey.backspace` and arrows become `SendEnigoKey` because they are not Unicode characters Enigo will type as text.

`TriggerMode` is WhileHeld for the send-key family (send-under-stick, `sendKey.*`) and Edge for toggles, paste, mode switches, layout switches, window flips/rotate, exit, `toggleRecord`, and completion actions (`cycleSuggestion`, `acceptSuggestion`, `cancelSuggestion`, …).

`do_action` is the keyboard’s interpreter:

- **Send under stick** records `last_*_stick_action` for the lock, then `send_key` on the `RawKey` of the highlighted cell if any. Default on the pads. Triggers use the same action when no chip is highlighted (`when` on the binding; see [bindings.md](bindings.md)).
- **SendKey / SendEnigoKey** call `send_key` with a synthetic `RawKey`.
- **ToggleShift / Ctrl / Alt** enqueue the corresponding `Event`; they do not flip state here.
- **Paste** enqueues Control-press, `v` click, Control-release as one `push_seq`, then disarms completion.
- **CycleSuggestion / CycleSuggestionPrev** move chip highlight (RB from none → slot 0; LB from none → last). No inject.
- **AcceptSuggestion** suffix-injects (or backspace-replaces) the highlighted or indexed chip.
- **ToggleCompletion** arms/disarms the typed log. Re-arm clears the log.
- **CancelSuggestion** clears highlight; with `retract_last_accept` also undoes the last accept: backspaces the injected text, retypes the original token when accept used backspace-replace, and requests chips for that token again. `l5` is `cancelSuggestion` when `suggestionJustAccepted` (else Backspace). `faceRight` is `cancelSuggestion` only while a chip is highlighted.
- **SwitchState / Flip / Rotate / Exit / ToggleRecord** enqueue the matching event.
- **SwitchLayout** changes `current_layout` immediately (not via the queue) and taps the recorder. A missing name is an error. The new layout drops captured geometry and recaptures on the next draw. Selection is remapped by screen position on the new board (not by row/column index). Analog mapping is biased by `origin1 − origin2` so the cursor stays put when rest centres differ; the bias lifts on idle. Until analog input moves, a different rest centre does not snap the highlight onto another key. The controller source that sent the switch is ignored until that button is released, so a still-held pad click or trigger does not type the key now under the stick (for example Shift after `switchLayout.symbols`). Other buttons and pad aiming stay live; thumbs can stay on the pads.

Layout-embedded actions (a key whose `key` field is `toggleShift`) go through `send_key` → `RawKey::Action` → the same `do_action`.

## Sending a key

`send_key` is where sticky modifiers become OS events.

For a character `RawKey::Key(c)` with **no** Ctrl/Alt/Shift sticky mods, the queue gets a single `SendText` of that character. Enigo text injection is what produces letters; a virtual-key click of `Unicode(c)` does not apply Shift for capitals in the way KOSK needs.

If any sticky mod is on, the queue gets a `push_seq`: press each held modifier, `SendKey(Unicode(c), Click)`, then release in reverse. Enigo’s text path ignores modifiers, so chords such as Ctrl+C must use virtual keys.

`RawKey::Enigo` (Backspace, arrows) always uses `SendKey` clicks, with the same modifier wrap if stickies are on.

`RawKey::Text` pushes `SendText` of the whole string (used when a layout entry is a multi-character literal).

If `push_seq` / `push` accepts the work, sticky mods and `shift_state` are cleared and the character is appended to the completion typed log (caret always at end). Arrows and paste disarm that log. If the queue drops the commit, modifiers stay so a later accept still applies them. That is why `send_key` checks the boolean from the queue.

## Shift versus sticky Shift

There are two Shift-related flags:

- **`shift_state`** chooses the layout layer: `key.normal` versus `key.shift` (or an automatic uppercase). The Shift key on the board looks selected when this is true. Toggling Shift when Ctrl/Alt are **off** flips `shift_state`.
- **`shift_mod`** is a sticky modifier for chords (Ctrl+Shift+C). Toggling Shift when Ctrl or Alt is already on flips `shift_mod` instead of the layer, so you can chord without switching the visible glyphs to the shifted layer.

`toggle_shift` in `process_events` implements that. Turning Shift off clears both flags. Ctrl and Alt are only sticky mods (`ctrl_mod`, `alt_mod`); they do not change the layer. Space can display a `ctrl+alt` overlay when `display_modifiers` is set on that key.

Caps-style layer Shift is what you use to type `A`. Sticky Shift is what you use with Ctrl.

## Drawing

`draw_keyboard_ui` walks the current layout’s rows, applies padding and scale, and builds an egui `Button` per key. Skip keys take up space without a widget. Appearance comes from `KeyButton::appearance` and a `DisplayContext` (shift layer, recording, replay, ctrl, alt, suggestion). Left highlight is blue, right is green, both sticks on one key is purple. The highlight is the selected **cell**, not every button that happens to show the same glyph. Idle sticks still show the rest cell (home-row `d` / `k` by convention in geometry) as a resting highlight.

When `[completion].enabled` and `show_in_keyboard`, a reserved chip strip is drawn **above** the keys (not stick-hittable, not in layout TOML) so key centres do not jump. See [completion.md](completion.md).

The first frame captures button centers from egui’s layout and stores them on the `KeyboardLayout` so hitboxes match what was drawn. Stick bounds are shifted by the same offset as that first key (chip strip, padding). Later frames recapture when those centers move. Debug overlays (cursors, hitboxes, bounds) are painted from the current geometry when `[debug]` is on.

A mouse click returns that key’s `RawKey` to `draw_ui`, which calls `send_key` with `MouseClick`.

## What this does not cover

**Hitbox math and TOML schema** are the layout document.

Completion is documented in [completion.md](completion.md). Chips sit above the keyboard and do not participate in stick hit-test.

## Summary

Keyboard mode maps two analog samples onto two highlighted layout cells, prefers the current cell until a neighbor is clearly closer, optionally freezes that highlight after a send, and turns bindings into `Event`s. Characters without modifiers go out as Unicode text; anything involving Ctrl, Alt, or sticky Shift goes out as a virtual-key chord in one `push_seq`. Layout Shift and sticky Shift are different flags so you can capitalize and chord without fighting the glyphs on screen.
