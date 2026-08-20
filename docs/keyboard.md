# On-screen keyboard (`keyboard/mod.rs`, `keyboard_action.rs`)

This document describes the keyboard mode: highlighting keys under the sticks, turning controller actions into events, and sticky modifiers. The types live in `src/state/keyboard/mod.rs` and `src/state/keyboard/keyboard_action.rs`. How a layout file becomes hitboxes is [keyboard-layout.md](keyboard-layout.md). How held triggers are throttled is [event-debounce.md](event-debounce.md).

## What this mode is

Keyboard mode is the default `StateId`. It draws the current layout as a grid of egui buttons, colors the keys nearest the left and right analog samples, and sends the highlighted key when a binding such as `sendKeyUnderLeftStick` fires. Mouse users can click a button instead; that path uses `EventSource::MouseClick` and the same `send_key` function.

`KeyboardState` is a process-wide singleton. `init` constructs it from config; `on_changed` reloads layouts and bindings. `AppState` talks to it through `keyboard::with_mut`.

## Stick selection

Each poll, `handle_controller_input` asks the current `KeyboardLayout` for the nearest selectable key to the left stick and to the right stick, given `shift_state` (so the highlighted glyph matches the shifted layer).

Those answers are not always applied immediately. After a stick-send action succeeds, that stick’s selection is frozen for `stick_select_lock_ms` (100 ms in the checked-in config). The lock exists so a small pad twitch right after a press does not slide onto a neighbor and type a different letter while the trigger is still down. When the lock expires, highlighting follows the stick again.

If the selected key on a side **does** change (lock not holding, or lock expired), the keyboard calls `events.clear_toggle_suppress` for that side’s usual sources (`triggerLeft` and `padLeft`, or the right pair). That is how you can hold a trigger, toggle Shift, then slide onto Ctrl and toggle Ctrl without releasing. Details are in the event-queue doc.

When the device yields `None` (idle), selection is cleared and the binding engine is reset.

## `KeyboardAction`

Mappings and on-layout keys both parse through `KeyboardAction::try_from`. Unit names are case-insensitive (`toggleShift`, `ToggleShift`). Parameterized names use a dot: `switchState.menu`, `switchLayout.symbols`, `sendKey.c`.

`sendKey.space`, `sendKey.enter`, `sendKey.tab` become character sends. `sendKey.backspace` and arrows become `SendEnigoKey` because they are not Unicode characters Enigo will type as text.

`TriggerMode` is WhileHeld for the send-key family (including send-under-stick) and Edge for toggles, paste, mode switches, layout switches, window flips/rotate, exit, and `toggleRecord`.

`do_action` is the keyboard’s interpreter:

- **Send under stick** records `last_*_stick_action` for the lock, then `send_key` on the highlighted `RawKey` if any.
- **SendKey / SendEnigoKey** call `send_key` with a synthetic `RawKey`.
- **ToggleShift / Ctrl / Alt** enqueue the corresponding `Event`; they do not flip state here.
- **Paste** enqueues Control-press, `v` click, Control-release as one `push_seq`.
- **SwitchState / Flip / Rotate / Exit / ToggleRecord** enqueue the matching event.
- **SwitchLayout** changes `current_layout` immediately (not via the queue), clears selection, and taps the recorder. A missing name is an error.

Layout-embedded actions (a key whose `key` field is `toggleShift`) go through `send_key` → `RawKey::Action` → the same `do_action`.

## Sending a key

`send_key` is where sticky modifiers become OS events.

For a character `RawKey::Key(c)` with **no** Ctrl/Alt/Shift sticky mods, the queue gets a single `SendText` of that character. Enigo text injection is what produces letters; a virtual-key click of `Unicode(c)` does not apply Shift for capitals in the way KOSK needs.

If any sticky mod is on, the queue gets a `push_seq`: press each held modifier, `SendKey(Unicode(c), Click)`, then release in reverse. Enigo’s text path ignores modifiers, so chords such as Ctrl+C must use virtual keys.

`RawKey::Enigo` (Backspace, arrows) always uses `SendKey` clicks, with the same modifier wrap if stickies are on.

`RawKey::Text` pushes `SendText` of the whole string (used when a layout entry is a multi-character literal).

If `push_seq` / `push` accepts the work, sticky mods and `shift_state` are cleared. If the queue drops the commit, modifiers stay so a later accept still applies them. That is why `send_key` checks the boolean from the queue.

## Shift versus sticky Shift

There are two Shift-related flags:

- **`shift_state`** chooses the layout layer: `key.normal` versus `key.shift` (or an automatic uppercase). The Shift key on the board looks selected when this is true. Toggling Shift when Ctrl/Alt are **off** flips `shift_state`.
- **`shift_mod`** is a sticky modifier for chords (Ctrl+Shift+C). Toggling Shift when Ctrl or Alt is already on flips `shift_mod` instead of the layer, so you can chord without switching the visible glyphs to the shifted layer.

`toggle_shift` in `process_events` implements that. Turning Shift off clears both flags. Ctrl and Alt are only sticky mods (`ctrl_mod`, `alt_mod`); they do not change the layer. Space can display a `ctrl+alt` overlay when `display_modifiers` is set on that key.

Caps-style layer Shift is what you use to type `A`. Sticky Shift is what you use with Ctrl.

## Drawing

`draw_keyboard_ui` walks the current layout’s rows, applies padding and scale, and builds an egui `Button` per key. Skip keys take up space without a widget. Appearance comes from `KeyButton::appearance` and a `DisplayContext` (shift layer, recording, replay, ctrl, alt). Left highlight is blue, right is green, both sticks on one key is purple. Idle sticks still show the center keys (`d` / `k` by convention in geometry) as a resting highlight.

The first frame captures button centers from egui’s layout and stores them on the `KeyboardLayout` so hitboxes match what was drawn. Debug overlays (cursors, hitboxes, bounds) are painted when `[debug]` is on.

A mouse click returns that key’s `RawKey` to `draw_ui`, which calls `send_key` with `MouseClick`.

## What this does not cover

**Hitbox math and TOML schema** are the layout document.

**The keyboard does not call the completion engine.** Suggestions are a separate crate module; text-input mode is the intended UI, and it is not wired yet.

## Summary

Keyboard mode maps two analog samples onto two highlighted keys, optionally freezes that highlight after a send, and turns bindings into `Event`s. Characters without modifiers go out as Unicode text; anything involving Ctrl, Alt, or sticky Shift goes out as a virtual-key chord in one `push_seq`. Layout Shift and sticky Shift are different flags so you can capitalize and chord without fighting the glyphs on screen.
