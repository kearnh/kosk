# Settings (`menu.rs`, `menu_action.rs`, `settings_form.rs`)

This document describes the settings screen. `StateId::Settings` is still one mode. The view stack lives in `settings_form.rs`: a hub, an Options list of shared pages plus per-device entries, a device index, and the value page itself. `menu.rs` draws that stack and applies controller input. `menu_action.rs` is the action enum those bindings name.

## What the screen shows

The keyboard opens settings with a chord. In the checked-in mappings, `share + faceTop` is `switchState.settings`.

The hub lists **Move window**, **Mappings**, **Layouts**, **Options**, and **Back**. Move window, mappings, and layouts switch to those modes. Options opens a second list: Suggestions, Overlay, Typing, one device entry per visible controller family, and Debug. Each device entry opens a device index, and each device index row opens a value page whose title names the device (`Steam Controller Stick`, `DualShock 4 Triggers`). A Steam Controller index has Pads, Stick, and Triggers; a DualShock 4 index has Stick and Triggers. Back, and the `back` action, return to the keyboard from the hub.

The list sits 16px in from the top and both sides. The screen title is white. The hub and the Options list are one column: the title, the rows, and a footer. The highlighted row is a full-width bar. On a value page the current value sits on the right of that row (`On`, `240 ms`, `1.25`), and a panel appears beside it. The panel title is white. The body is the explainer for that value, including a single sentence. Hub and category rows have no panel. Nothing is drawn under the list.

The footer draws each bound button as a 16px glyph, then a short word. A remapped button shows that button's glyph.

Labels name the thing being set (`Thumb rest`, `Delay before repeat`). The TOML key is not shown. The device entry names the family up front, so there is never a bare `Controller` row.

Each value page edits one device profile. Stick rows write that device's stick profile (`sc2.stick` or `ps4.stick`), even when the other family is connected. A Triggers page is the two trigger cutoffs for that family. The Steam Controller Pads page holds pad aim plus pad feel: thumb rest, short-side stretch, and pad click. Pad click writes both pads. Replay edits the Steam Controller profiles, matching the tape-family fallback in `config::resolved_controller`.

By default Options lists only the connected family's device entry. The Options footer has a `show other controllers` hint on its own line, bound to `toggleShowAllControllers`; activating it shows both Steam Controller and DualShock 4 and the hint becomes `hide other controllers` for the rest of the session. The flag is not written to `config.toml`.

Debug rows insert a `[debug]` table if one was missing. The table’s presence is what turns debug drawing on.

Paths, model weights, colors, and word lists are not on this screen. They stay in `config.toml`.

## Mouse

A click on the hub or the category list opens that row. On a value page, the first click on a row only moves the highlight, so the panel follows it. A second click on an already highlighted toggle flips it. Steppers do not change on click; left and right do.

## Controller

`MenuAction` is entirely `TriggerMode::Edge`: `selectUp`, `selectDown`, `selectLeft`, `selectRight`, `activate`, `pagePrev`, `pageNext`, `back`, `openConfig`, `toggleShowAllControllers`, and `switchState.*`.

Up and down wrap the current list. Left and right change the highlighted value: toggles flip, choices wrap, numbers step and stop at their ends. Activate opens a hub, Options, or device-index row, and flips a toggle. `pagePrev` and `pageNext` change value page only while a value page is open; they cycle the filtered sequence (shared pages plus the visible device pages) and wrap from Debug back to Suggestions. `back` goes up one level: value page to device index to Options to hub. From the hub it returns to the keyboard. `toggleShowAllControllers` flips the session-only device filter on the Options list and does not touch the config file.

The checked-in map binds `faceRight` to `back`, `faceTop` to `toggleShowAllControllers`, and `share` to `openConfig`. On a Steam Controller, `share` is the left button beside Steam. On the Options list the footer shows that button as "open config", unless a config path was given on the command line, in which case the hint is hidden and the action does nothing. `openConfig` opens `%LOCALAPPDATA%\kosk\config.toml` in `$EDITOR` when that is set, otherwise Notepad, creating the file when it is missing. `switchState.keyboard` is still a valid action and leaves for the keyboard immediately. The left stick is emulated as a d-pad, so it moves the highlight and adjusts values the same way.

Idle `None` input resets the binding engine so edges do not fire on the next reconnect.

## Saving

Left and right write the in-memory config only. Opacity, warp, debounce, and the debug flags are read from that config on later frames, so they move before the file does. The file is written when you leave a page that changed, including when L1 or R1 changes page, and when `switchState` leaves settings with unsaved edits. That write then notifies listeners once. Notifying on every step would re-read every layout file and respawn word suggestions. Key width and stick range are stored in the loaded layouts, so they show up when that reload runs, which is when you leave the page.

While a tape config is overlaid, the save copies only the fields you changed onto the on-disk snapshot. Tape-merged values that you did not edit stay off the file. Move window still saves position through the existing path, which during an overlay writes only `window_pos`.

## What this does not cover

**Move-window analog placement is not this file.** Settings only switches into that mode. Stick and pad motion once you are there is [window-position.md](window-position.md).

**Binding tables are not edited here.** That is the mappings screen. See [bindings.md](bindings.md).

## Summary

Settings is a hub, an Options list with per-device entries, device indexes, and short pages of values. The explainer panel appears only beside a highlighted value. Up and down move, left and right change the value, and back climbs one level until it returns to the keyboard. Edits sit in memory until you leave the page, and then one write updates the file.
