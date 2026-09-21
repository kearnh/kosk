# Binding engine (`bindings.rs`, `actions.rs`)

This document describes how a controller snapshot becomes a list of typed actions. `src/controller/bindings.rs` implements `BindingEngine`. `src/state/actions.rs` defines `Action`, `TriggerMode`, and `load_bindings`. Each mode has its own action enum (`KeyboardAction`, `MenuAction`, and so on); those enums are documented with the mode that runs them.

## What problem does this solve?

`mappings.toml` says things like `"triggerLeft" = "sendKeyUnderLeftStick"` and `"options + faceTop" = "switchState.settings"`. A few keys mean two things: accept a highlighted chip, or type. Those use a `when` clause on the same key, not a composite action name. The keyboard should not parse TOML on every poll. At init (and on config reload) each mode asks `load_bindings` for a `BindingEngine<ThatMode's Action>`. Every controller poll, the engine returns which mappings fired.

Two complications sit in that sentence. Some actions should run on the rising edge only (open settings once). Others should run on every poll while the button is held (type the highlighted letter, with debounce later). And two-button chords have to coexist with single-button mappings on the follower without firing both.

## File shape

Each mode is still a table (`[Keyboard]`, `[Settings]`, …). A value is one of:

- a string: always that action.
- `{ action = "…", when = "…" }`: fire only when the clause is true (`faceRight` cancel while a chip is highlighted).
- an array of those tables: first entry whose `when` is missing or true wins. The entry without `when` is the fallback and must be last.

TOML forbids the same key twice, so two meanings for one button live in that array.

The user-facing list of button names, action names, and flags is [MAPPINGS.md](../MAPPINGS.md).

`when` strings parse in `src/when.rs` (same language as layout display). Canonical flags:

| Flag | True when |
|------|-----------|
| `suggestionSelected` | a completion chip is highlighted |
| `completionActive` | completion is predicting (armed) |
| `suggestionJustAccepted` | a suggestion chip was just accepted and nothing has been typed since |
| `modifier` | any of shift / ctrl / alt |
| `modifier.shift` `modifier.ctrl` `modifier.alt` | that sticky modifier |
| `recording` `replay` | tape record / playback |

`shift` / `ctrl` / `alt` still parse as aliases of `modifier.*`. Unknown names fail at load. `suggestion` and `armed` are not flags.

`load_bindings` compiles a value into `BindingTarget::Always(A)` or `BindingTarget::Conditional { arms, otherwise }`. Old names `sendKeyUnderLeftStickOrAcceptSuggestion`, `sendKeyUnderRightStickOrAcceptSuggestion`, and `enterOrAcceptSuggestion` still expand to the equivalent array (accept when `suggestionSelected`, else type / Enter / `submit`).

## Actions and trigger modes

`Action` is a small trait: `as_any` for downcasting, and `trigger_mode`.

`TriggerMode::Edge` means “fire when this button becomes down.” `TriggerMode::WhileHeld` means “fire every evaluation while the button is physically down.” Hold-repeat for typing is **not** implemented here. WhileHeld actions still run at poll rate; [the event queue](event-debounce.md) drops extras.

`get_action(state, name)` parses a mapping value in the context of a `StateId`. `"toggleShift"` is a `KeyboardAction` only in the Keyboard table. The same string in `[Settings]` would not parse. `load_bindings` walks `config.controller_map` for that state, compiles entries whose action names parse as the requested type `A`, and builds the engine. Unknown names in a rule are skipped; a bad `when` or fallback-not-last aborts that mode’s map.

## Building the engine

`BindingEngine::try_from_raw` splits the map into singles and chords. A button that is a chord **leader** must not also have a standalone mapping. That rule is load-time: `"options" = "something"` together with `"options + faceTop" = "switchState.settings"` is an error. The **follower** may have its own single mapping. In the checked-in file, `faceTop` toggles Shift, and `options + faceTop` still opens settings because of the suppress rule below.

The engine stores the set of buttons that appear in any mapping so each poll only queries those buttons.

## Evaluation

`evaluate` takes a controller snapshot and a `WhenContext` (Keyboard and TextInput fill `suggestionSelected` and modifiers; Settings and the others pass defaults).

When a snapshot is present:

1. Compute `held` as the mapped buttons that `query` as down.
2. `newly_down` / `newly_up` are differences against `prev_held`.
3. Buttons that went up leave `suppress_single` and any `Conditional` latch.
4. Leaders that are down are recorded in `leaders_active`.
5. Chord pairs that are no longer both down leave `chords_fired`.
6. **Chords first.** If the follower is newly down and the leader is already active, and this chord has not already fired for this hold, the chord’s `BindingTarget` is resolved and emitted, the pair is marked fired, and the follower is added to `suppress_single`.
7. **Singles second.** `Always` uses that action’s trigger mode. `Conditional` latches the resolved action on press (including “no match”) until release, then uses *that* action’s trigger mode. That is how accept-on-trigger (Edge) does not turn into type-under-stick (WhileHeld) on the next poll after the highlight clears.

Chords are therefore leader-first: hold Options, then tap Triangle, and you get the chord rather than ToggleShift. If you tap Triangle alone, Options is not active, the chord does not fire, and the single mapping runs. If you complete a chord, Triangle stays suppressed until you release it, so you do not also toggle Shift on the same press.

`chords_fired` prevents the chord from repeating every poll while both buttons stay down. Chord actions in this codebase are Edge-style mode switches, so that is the intended feel. There is no WhileHeld chord path.

WhileHeld singles (the stick-send actions, cursor moves) emit every poll for as long as the button is in `held` and not suppressed. That is the stream `EventQueue` later throttles.

Idle disconnect still goes through `reset`, not `evaluate(None)`.

## How a mode uses the result

Keyboard, settings, move-window, and text-input each loop the returned `(ControllerBinding, Action)` pairs, wrap the binding in `EventSource::Controller`, and call `do_action`. The binding is the debounce bucket: `triggerLeft` and `padLeft` are different sources even if both send the key under the left stick.

Mouse clicks never go through `BindingEngine`. They push events with `EventSource::MouseClick` directly.

## What this does not cover

**The engine does not know about layouts or Enigo.** It only produces action enums after resolving `when`. Keyboard `do_action` decides whether to enqueue `SendText` or `ToggleShift`.

**Debounce is downstream.** If a WhileHeld mapping feels like it auto-repeats too fast or too slow, look at `EventQueue` and `event_debounce_*`, not at this file.

**Three-button chords do not exist.** A spec with two `+` signs fails to parse.

## Summary

`BindingEngine` is the per-mode interpreter of `controller_map`. It distinguishes rising-edge actions from held actions, leader-first two-button chords, and `when` branches latched for the hold. It still fires WhileHeld mappings at poll rate; the event queue is the layer that makes holding a trigger feel like a keyboard instead of a machine gun.
