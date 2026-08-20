# Text input (`text_input.rs`, `text_input_action.rs`)

This document describes text-input mode in `src/state/text_input.rs` and `src/state/text_input_action.rs`. In this mode the on-screen keyboard still draws, but letters do not go to the focused application until you submit the line. Word completion is **not** called from here yet; the engine is documented in [completion.md](completion.md) and the intended UX in [plans/completion.md](plans/completion.md).

## What problem does this solve?

Injecting every character immediately is right for typing into a game or a browser bar. It is awkward for composing a longer word when you might backspace often: each mistake has already landed in the other application. Text-input mode keeps a local buffer, shows it in a single-line field, and on submit sends the whole string plus Return, then returns to keyboard mode.

## The nested event queue

`TextInputState` has its own `EventQueue` named `kb_events`, plus the same kind of `BindingEngine<TextInputAction>` other modes have.

On draw, it paints a non-interactive `TextEdit` (egui is not allowed to type into it; the caret is drawn by hand from `cursor_pos`) using `[text_input]` colors and font size. Then it calls `keyboard::draw_ui` with **`kb_events`**, not the app-wide queue. Mouse clicks on keys therefore land in the nested queue.

`process_events` on the text-input state drains `kb_events` and interprets a subset:

- `SendKey(Unicode(ch), Click)` inserts that character at `cursor_pos`.
- `SendText` inserts each character.
- `Backspace` click deletes the character before the cursor.
- `Return` click submits (see below).
- Any other event is forwarded to the **outer** queue with the same `EventSource`. Toggles, mode switches, recording, and window flips still reach `AppState`.

Modifier wrap from the keyboard (Control press/click/release) is ignored for Unicode clicks except that the nested queue still has to accept the whole `push_seq`. Only the Unicode click and `SendText` change the buffer. Ctrl+C in this mode will not copy inside the field; the chord is mostly swallowed except for forwarded non-text events.

Submit builds a `push_seq` on the **outer** queue: `SendText` of the buffer, a Return click if the buffer was non-empty, then `ChangeState(Keyboard)`. An empty submit still returns to the keyboard without typing.

## Controller handling

`TextInputAction` today is `moveCursorLeft`, `moveCursorRight` (WhileHeld, so debounce applies), and `switchState.*` (Edge). The sample map uses d-pad left/right for the caret and d-pad up to go back to the keyboard without submitting.

If **any** text-input binding fires on a poll, the keyboard is not given that snapshot. That keeps a d-pad left that means “caret” from also meaning “arrow key” on the keyboard layer. If nothing in the text-input map fires, the snapshot is forwarded to `KeyboardState::handle_controller_input` with `kb_events`, so triggers still type into the buffer through the same path as mouse clicks.

After that, `kb_events.end_controller_tick()` runs. `process_events` runs on this path only when the text-input map did not fire, so a cursor move does not also drain a stick-send from the same poll. `AppState` still ends and drains the **outer** queue, which is where submit and forwarded events live.

Idle `None` resets the text-input binding engine.

## Cursor

`cursor_pos` is a **byte** index into the UTF-8 buffer, matching `split_at_cursor` in the completion module. Insert and backspace currently treat that index as if it were a character boundary and move by one byte, which is correct for ASCII and will mis-handle multi-byte characters. The painted caret converts the byte index to a character index for egui’s `CCursor`.

The field is `interactive(false)` so a mouse click on the field does not move the caret; only the d-pad actions and typing do.

## What this does not cover

**Suggestions, chips, and accept/cancel actions are not implemented.** The completion engine can already rank prefixes; this mode never constructs a `CompletionContext`.

**Paste does not read the clipboard into the field.** The keyboard’s paste action is a Control+V sequence. This mode ignores Control press and release, then treats the `v` click as an ordinary character, so the buffer gains the letter `v`. There is no dedicated “insert clipboard” action.

## Summary

Text-input mode reuses the on-screen keyboard but aims its events at a local string. A nested `EventQueue` intercepts letters and backspace; everything else is forwarded. Submit types the line into the focused application and returns to keyboard mode. Cursor moves are a small action set that preempts the keyboard for that poll. Completion remains a library sitting next to this mode, not inside it.
