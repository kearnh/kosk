# Mappings editor (`mappings.rs`, `select_key.rs`, `mapping.rs`)

This document describes the in-app bindings editor: the Mappings screen where you remap buttons without hand-editing TOML, and the key picker it opens on top. The editor lives in `src/state/mappings.rs`, the picker in `src/state/select_key.rs`, and the shared pill and rule types in `src/controller/mapping.rs` (`MappingPill`, `MappingRule`, `MappingValue`, `validate_rule_order`). Which strings are legal buttons and actions is the vocabulary in [MAPPINGS.md](../../MAPPINGS.md); how those bindings fire at runtime is the engine in [bindings.md](bindings.md). The call-stack machinery the two screens use is in [app-state.md](app-state.md).

## Why an editor exists

`mappings.toml` is easy to break by hand: a chord leader that also has a single mapping, a duplicated binding, or a `when` fallback in the wrong position each silently change what a button does (or stop a whole mode's map from loading). The editor exists so a person holding a controller can remap buttons with those mistakes caught at edit time, see every action a mode offers, and save without touching a text file. It is action-centric rather than button-centric: each row is an action, and each action holds one or more binding pills.

## Entering and leaving

The settings hub opens the editor with its Mappings row, which switches to `StateId::Mappings` and starts a fresh session: the draft is reloaded from config, the dirty flag clears, and focus parks on the Keyboard tab's table. Leaving without saving discards the draft; there is no cross-session memory of it. Cancel returns to Settings. Save validates every tab, writes the user `mappings.toml` through `config::save`, notifies listeners so the new bindings take effect, and reports `saved` in the status line.

## The draft and the rows

The draft is one table per editable mode — Keyboard, Settings, SelectLayout, TextInput, and MoveWindow — mapping each action name to its list of pills. A pill is one binding plus its optional `when` clause. Loading the draft skips bindings whose actions this build does not understand and says so in the status line (`warning: N unknown actions ignored`), so the editor never shows a row it could not save back faithfully.

Rows come from a catalog per mode: the action enum's variants in camel case plus `switchState.*` entries, with Keyboard additionally listing one row per layout for `switchLayout.*`, the concrete `sendKey.*` bindings already in the draft, and a `sendKey` gateway row for typing a fresh character send. The Mappings and SelectKey modes have no catalog because they cannot be remapped. On the TextInput tab a note explains that unmapped inputs inherit Keyboard bindings — that is runtime behavior, not table inheritance: text-input mode forwards any snapshot none of its own bindings claimed to the keyboard layer, so a button with no TextInput row still types through the Keyboard map.

## Browsing with a controller

The editor's own buttons are hardcoded at the top of `mappings.rs` and are not remappable: the d-pad moves focus, the shoulders switch mode tabs, A activates, Y asks to delete, and B goes back or cancels. The left stick also nudges focus through the shared stick-as-dpad helper. Focus moves through zones — mode tabs, the table, Cancel, Save — and inside the table through rows and then pills, with a trailing `+` control on every row for adding a binding. Pressing A on a pill opens the key picker to replace that pill; pressing A on `+` opens it to add one. Pressing Y on a pill opens a small delete dialog with Cancel focused, where left and right choose the button and A confirms. Mouse users can click tabs, rows, pills, and Save or Cancel directly; clicking a pill opens the same picker a controller press would.

Entering the editor while a button is held does not fire anything: the mode resets its edge state against the currently held buttons, and the same holdover reset runs when the picker opens and closes, so the A press that opened the picker cannot also confirm inside it.

## The key picker

The picker is a temporary text-entry screen, and it says so in its own heading. It shows two fields — the binding and the action — prefilled with the pill being replaced or with the row's action when adding. Under the action field it lists the bindings already on that action, excluding the pill under edit, so you can see at a glance whether you are duplicating something. A on the controller submits (OK) and B cancels; the mouse equivalents are buttons. Submitting validates locally first: the binding must parse as a button or chord, and the action must exist for the calling mode (a bare `sendKey.` gateway with no key is rejected here). Success returns the canonical binding string and the trimmed action to the editor; failure stays on the picker with a status message.

The picker runs as a sub-UI call: the editor enqueues `CallState` with the prefill, the app pushes the editor onto the call stack and shows the picker, and the picker's answer comes back as `ReturnState`, which the editor applies in `on_return`. A cancellation leaves the draft untouched and needs no repaint.

## Validation and saving

Every replacement and every addition is checked against the tab's draft before it lands. The rules are the same ones the binding engine enforces at load: a binding with the same `when` cannot already belong to another action, a chord's leader and follower must differ, a button that leads a chord cannot also have a single mapping, and a button with a single mapping cannot become a chord leader. A chord follower may still have its own single mapping elsewhere. A conflict leaves the draft unchanged and names the problem in the status line (`conflict: faceTop already bound to toggleShift`).

Save revalidates all five tabs and converts the draft back into mapping values, sorting `when` rules before the fallback and rejecting a fallback that is not last. Unknown actions or bindings this build never understood are carried over from the live map untouched, so saving never drops a future version's rows. A changed binding is written to the user `mappings.toml`, a removed default binding is written as `"none"`, and a binding that matches the default again is removed. The save then calls `notify_changed` so every mode reloads its engine immediately.

## What this does not cover

This page does not cover what happens when a saved binding fires; that is the engine in [bindings.md](bindings.md) and the per-mode `do_action` handlers. It does not cover the `when` language itself; that is shared with layouts and described in [keyboard-layout.md](keyboard-layout.md). The on-disk merge of user mappings onto built-in defaults is config's job; see [config.md](config.md).

## Summary

The mappings editor is a draft over the five remappable modes, browsed by fixed controller buttons and edited through a key-picker sub-screen. Every change is validated against the engine's own conflict rules before it lands, unknown rows are preserved rather than dropped, and saving writes the user sidecar and reloads all bindings at once. The picker exists only to collect one binding string and hand it back; the editor owns the draft, the validation, and the save.
