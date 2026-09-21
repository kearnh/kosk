# Settings (`menu.rs`, `menu_action.rs`, `settings_form.rs`)

This document describes the settings screen. `StateId::Settings` is still one mode. The view stack lives in `settings_form.rs`: a hub, a list of option pages, and the page itself. `menu.rs` draws that stack and applies controller input. `menu_action.rs` is the action enum those bindings name.

## What the screen shows

The keyboard opens settings with a chord. In the checked-in mappings, `options + faceTop` is `switchState.settings`.

The hub lists **Move window**, **Mappings**, **Layouts**, **Options**, and **Back**. Move window, mappings, and layouts switch to those modes. Options opens a second list: Suggestions, Overlay, Typing, Sticks, Controller, and Debug. Each of those is a short page of values. Back, and the `back` action, return to the keyboard from the hub.

The window is two columns, and both stay the same width on every row. The left column is the title, the rows, and a footer that names the buttons. The highlighted row is a full-width bar. On a value page the current value sits on the right of that row (`On`, `240 ms`, `1.25`). The right column is a panel: the row’s name, then the explainer. Every row has one, including a single sentence. Nothing is drawn under the list.

Labels name the thing being set (`Thumb rest`, `Delay before repeat`). The TOML key is not shown.

Controller rows follow the connected device. A DualShock 4 page is the two trigger cutoffs. A Steam Controller 2 page, and replay, also has thumb rest, short-side stretch, and pad click. Pad click writes both pads.

Debug rows insert a `[debug]` table if one was missing. The table’s presence is what turns debug drawing on.

Paths, model weights, colors, and word lists are not on this screen. They stay in `config.toml`.

## Mouse

A click on the hub or the category list opens that row. On a value page, a click moves the highlight so the panel updates. A second click on a toggle flips it. Steppers do not change on click; left and right do.

## Controller

`MenuAction` is entirely `TriggerMode::Edge`: `selectUp`, `selectDown`, `selectLeft`, `selectRight`, `activate`, `pagePrev`, `pageNext`, `back`, and `switchState.*`.

Up and down wrap the current list. Left and right change the highlighted value: toggles flip, choices wrap, numbers step and stop at their ends. Activate opens a hub or category row, and flips a toggle. `pagePrev` and `pageNext` change page only while a page is open; they wrap from Debug back to Suggestions. `back` goes up one level. From the hub it returns to the keyboard.

The checked-in map binds `faceRight` to `back`. `switchState.keyboard` is still a valid action and leaves for the keyboard immediately. The left stick is emulated as a d-pad, so it moves the highlight and adjusts values the same way.

Idle `None` input resets the binding engine so edges do not fire on the next reconnect.

## Saving

Left and right write the in-memory config only. Opacity, warp, debounce, and the debug flags are read from that config on later frames, so they move before the file does. The file is written when you leave a page that changed, including when L1 or R1 changes page, and when `switchState` leaves settings with unsaved edits. That write then notifies listeners once. Notifying on every step would re-read every layout file and respawn word suggestions. Key width and stick range are stored in the loaded layouts, so they show up when that reload runs, which is when you leave the page.

While a tape config is overlaid, the save copies only the fields you changed onto the on-disk snapshot. Tape-merged values that you did not edit stay off the file. Move window still saves position through the existing path, which during an overlay writes only `window_pos`.

## What this does not cover

**Move-window analog placement is not this file.** Settings only switches into that mode. Stick and pad motion once you are there is [window-position.md](window-position.md).

**Binding tables are not edited here.** That is the mappings screen. See [bindings.md](bindings.md).

## Summary

Settings is a hub plus short pages of values, with a panel beside the highlighted row. Up and down move, left and right change the value, and back climbs one level until it returns to the keyboard. Edits sit in memory until you leave the page, and then one write updates the file.
