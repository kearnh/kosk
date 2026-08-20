# Binding engine (`bindings.rs`, `actions.rs`)

This document describes how a controller snapshot becomes a list of typed actions. `src/controller/bindings.rs` implements `BindingEngine`. `src/state/actions.rs` defines `Action`, `TriggerMode`, and `load_bindings`. Each mode has its own action enum (`KeyboardAction`, `MenuAction`, and so on); those enums are documented with the mode that runs them.

## What problem does this solve?

`mappings.toml` says things like `"triggerLeft" = "sendKeyUnderLeftStick"` and `"options + faceTop" = "switchState.menu"`. The keyboard should not parse TOML on every poll. At init (and on config reload) each mode asks `load_bindings` for a `BindingEngine<ThatMode's Action>`. Every controller poll, the engine returns which mappings fired.

Two complications sit in that sentence. Some actions should run on the rising edge only (open the menu once). Others should run on every poll while the button is held (type the highlighted letter, with debounce later). And two-button chords have to coexist with single-button mappings on the follower without firing both.

## Actions and trigger modes

`Action` is a small trait: `as_any` for downcasting, and `trigger_mode`.

`TriggerMode::Edge` means “fire when this button becomes down.” `TriggerMode::WhileHeld` means “fire every evaluation while the button is physically down.” Hold-repeat for typing is **not** implemented here. WhileHeld actions still run at poll rate; [the event queue](event-debounce.md) drops extras.

`get_action(state, name)` parses a mapping value in the context of a `StateId`. `"toggleShift"` is a `KeyboardAction` only in the Keyboard table. The same string in `[Menu]` would not parse. `load_bindings` walks `config.controller_map` for that state, keeps entries whose names parse as the requested type `A`, and builds the engine. Unknown names are skipped rather than aborting the whole map.

## Building the engine

`BindingEngine::try_from_raw` splits the map into singles and chords. A button that is a chord **leader** must not also have a standalone mapping. That rule is load-time: `"options" = "something"` together with `"options + faceTop" = "switchState.menu"` is an error. The **follower** may have its own single mapping. In the checked-in file, `faceTop` toggles Shift, and `options + faceTop` still opens the menu because of the suppress rule below.

The engine stores the set of buttons that appear in any mapping so each poll only queries those buttons.

## Evaluation

`evaluate` takes `Option<&dyn ControllerInput>`. `None` means the device went idle or disconnected: the engine resets edge state and returns no actions. That is how a stuck “held” mapping does not keep firing after the pad is gone.

When a snapshot is present:

1. Compute `held` as the mapped buttons that `query` as down.
2. `newly_down` / `newly_up` are differences against `prev_held`.
3. Buttons that went up leave `suppress_single`.
4. Leaders that are down are recorded in `leaders_active`.
5. Chord pairs that are no longer both down leave `chords_fired`.
6. **Chords first.** If the follower is newly down and the leader is already active, and this chord has not already fired for this hold, the chord action is emitted, the pair is marked fired, and the follower is added to `suppress_single`.
7. **Singles second.** A single mapping fires when its trigger mode says so, unless that button is in `suppress_single`.

Chords are therefore leader-first: hold Options, then tap Triangle, and you get the chord rather than ToggleShift. If you tap Triangle alone, Options is not active, the chord does not fire, and the single mapping runs. If you complete a chord, Triangle stays suppressed until you release it, so you do not also toggle Shift on the same press.

`chords_fired` prevents the chord from repeating every poll while both buttons stay down. Chord actions in this codebase are Edge-style mode switches, so that is the intended feel. There is no WhileHeld chord path.

WhileHeld singles (the stick-send actions, nudges, cursor moves) emit every poll for as long as the button is in `held` and not suppressed. That is the stream `EventQueue` later throttles.

## How a mode uses the result

Keyboard, menu, move-window, and text-input each loop the returned `(ControllerBinding, Action)` pairs, wrap the binding in `EventSource::Controller`, and call `do_action`. The binding is the debounce bucket: `triggerLeft` and `padLeft` are different sources even if both send the key under the left stick.

Mouse clicks never go through `BindingEngine`. They push events with `EventSource::MouseClick` directly.

## What this does not cover

**The engine does not know about Shift, layouts, or Enigo.** It only produces action enums. Keyboard `do_action` decides whether to enqueue `SendText` or `ToggleShift`.

**Debounce is downstream.** If a WhileHeld mapping feels like it auto-repeats too fast or too slow, look at `EventQueue` and `event_debounce_*`, not at this file.

**Three-button chords do not exist.** A spec with two `+` signs fails to parse.

## Summary

`BindingEngine` is the per-mode interpreter of `controller_map`. It distinguishes rising-edge actions from held actions, and it implements leader-first two-button chords by suppressing the follower’s single mapping until release. It still fires WhileHeld mappings at poll rate; the event queue is the layer that makes holding a trigger feel like a keyboard instead of a machine gun.
