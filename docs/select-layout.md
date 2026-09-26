# Layout picker (`select_layout.rs`, `select_layout_action.rs`)

This document describes the layout picker: the small screen that lists keyboard layouts and optionally previews one before switching. The state lives in `src/state/select_layout.rs` and its action enum in `src/state/select_layout_action.rs`. How layouts themselves work is in [keyboard-layout.md](keyboard-layout.md); switching layouts from the keyboard is in [keyboard.md](keyboard.md).

## What this mode is

The picker answers one question — which named layout should the keyboard draw — without making you memorize layout names for `switchLayout.*` bindings. The settings hub opens it with its Layouts row, which refreshes the list and highlights the current layout. The list itself is the names of the loaded keyboard layouts, with the active one marked `(current)`. Activating a row makes that layout current immediately and returns to the keyboard.

## Browsing and preview

`SelectLayoutAction` is entirely edge-triggered: `selectUp` and `selectDown` move the highlight with wraparound, `activate` switches, `togglePreview` shows or hides a live preview, and `switchState.*` leaves. The left stick also moves the highlight through the shared stick-as-dpad helper. Mouse users click a row, which selects and activates it in one step.

Preview renders a second, off-screen keyboard beside the list using a copy of the config pointed at the highlighted layout. It is a copy rather than the real keyboard so that browsing previews never disturbs the live board: highlighting another row only repoints the copy. Typing into the preview is not possible; it is drawn, not handled. Toggling preview off drops the copy.

## Switching

Activating writes the highlighted name into the real keyboard's current layout, taps the recording session so tapes mark the switch, and enqueues a change back to keyboard mode. A layout that fails to apply prints an error and stays on the picker rather than leaving the keyboard in a half-switched state. Like every mode switch, the move resets controller edge state against held buttons, so the button that activated the switch cannot also type on the freshly shown board.

## What this does not cover

Hitbox math, display rules, and the TOML schema are the layout document's job. The `switchLayout.*` keyboard actions that bypass this screen are covered with the keyboard. Recording headers that store layout sources for replay are in [record-replay.md](record-replay.md).

## Summary

The layout picker is a list with an optional live preview copy. Browsing never touches the real keyboard; only activating commits, and committing records the switch on the tape before returning to the keyboard.
