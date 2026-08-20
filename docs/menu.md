# Menu (`menu.rs`, `menu_action.rs`)

This document describes the menu mode in `src/state/menu.rs` and `src/state/menu_action.rs`. It is a small, unfinished screen: two buttons, a selected index, and controller bindings to move and activate. [todo.md](../todo.md) still lists richer settings UI and other work that is not present.

## What the menu is today

`StateId::Menu` is reached from the keyboard with a chord (in the checked-in mappings, `options + faceTop` is `switchState.menu`). The screen shows two egui buttons, **Move** and **Back**. Move switches to `MoveWindow`. Back switches to `Keyboard`. There is no config editor, no layout picker, and no overlay of the keyboard behind the buttons.

`MenuState` holds the button list (text plus a callback that returns an optional `StateId`), the `selected` index, and a `BindingEngine<MenuAction>`. Bindings reload on config change like the other modes.

## Mouse

`draw_ui` draws each button with `selected(true)` when that row is the controller selection, so the highlight is visible even if you only use the mouse. A click runs that button’s callback and enqueues `ChangeState` with `EventSource::MouseClick`.

## Controller

`MenuAction` is entirely `TriggerMode::Edge`: `selectUp`, `selectDown`, `activate`, and `switchState.*`. D-pad up/down wrap around the list with `rem_euclid`. Activate runs the callback for `buttons[selected]`. `switchState.keyboard` (bound to `faceRight` in the sample map) leaves without using the highlighted row, which is the usual “cancel” path.

There is no analog-stick highlight. Idle `None` input resets the binding engine so edges do not fire on the next reconnect.

## What this does not cover

**Move-window controller navigation is not this file.** The menu only switches into that mode. D-pad behavior once you are there is [window-position.md](window-position.md).

**This is not a settings page.** Changing debounce, mappings, or transparency still means editing TOML.

## Summary

The menu is a two-item switcher with wrapping d-pad selection and click-to-activate. It exists so you can reach window placement without a keyboard mapping, and it is intentionally thin compared to the rest of the app.
