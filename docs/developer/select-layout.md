# Layout picker (`select_layout.rs`, `select_layout_action.rs`)

This document describes the layout picker. The state lives in `src/state/select_layout.rs` and its action enum in `src/state/select_layout_action.rs`. How layouts themselves work is in [keyboard-layout.md](keyboard-layout.md); switching layouts from the keyboard is in [keyboard.md](keyboard.md).

## What this mode is

The settings hub opens the picker with its Layouts row, which refreshes the list and highlights the current layout. The list shows loaded layout names, with the active one marked `current`. It shares row styling, heading, spacing, and controller hints with settings. Activating a row makes that layout current and returns to the keyboard.

## Browsing

`SelectLayoutAction` is entirely edge-triggered: `selectUp` and `selectDown` move the highlight with wraparound, `activate` switches, and `switchState.*` leaves. The left stick also moves the highlight through the shared stick-as-dpad helper. Mouse users click a row, which selects and activates it in one step.

## Switching

Activating writes the highlighted name into the real keyboard's current layout, taps the recording session so tapes mark the switch, and enqueues a change back to keyboard mode. A layout that fails to apply prints an error and stays on the picker rather than leaving the keyboard in a half-switched state. Like every mode switch, the move resets controller edge state against held buttons, so the button that activated the switch cannot also type on the freshly shown board.

## What this does not cover

Hitbox math, display rules, and the TOML schema are the layout document's job. The `switchLayout.*` keyboard actions that bypass this screen are covered with the keyboard. Recording headers that store layout sources for replay are in [record-replay.md](record-replay.md).

## Summary

Browsing leaves the keyboard unchanged. Activating records the layout switch before returning to the keyboard.
