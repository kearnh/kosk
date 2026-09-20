# Controller mappings

This page is the vocabulary for `mappings.toml`: button names, action names, and `when` conditions. Point at that file from `config.toml` with `controller_map`; the path is relative to the config file.

Each `[Heading]` is a screen of the app. A line maps a button (left of `=`) to an action on that screen (right of `=`). Names ignore case, and hyphens and underscores do not matter, so `faceBottom`, `face-bottom`, and `face_bottom` are the same button.

Two buttons joined with `+` are a chord. Hold the first, then press the second. The first button cannot also have a mapping of its own in that table; the second one can.

One button can do different things in different situations. Write a list; the first entry whose `when` condition is true (or that has no `when`) is used. Put the default last, with no `when`. A single `{ action, when }` with no fallback does nothing when the condition is false.

The shipped file is the working example.

## Buttons

| Name | DualShock 4 | Steam Controller 2 |
|------|-------------|--------------------|
| `dpadUp` `dpadDown` `dpadLeft` `dpadRight` | D-pad | D-pad |
| `faceBottom` | Cross | A |
| `faceRight` | Circle | B |
| `faceLeft` | Square | X |
| `faceTop` | Triangle | Y |
| `shoulderLeft` `shoulderRight` | L1 / R1 | L1 / R1 |
| `triggerLeft` `triggerRight` | L2 / R2 (past the threshold in `config.toml`) | L2 / R2 |
| `stickLeft` `stickRight` | L3 / R3 (stick click) | stick click |
| `options` | Options | Start / Menu |
| `share` | Share | |
| `system` | PS | Steam |
| `padLeft` `padRight` | unused | left / right pad click |
| `l4` `l5` `r4` `r5` | unused | paddles |
| `quickAccess` | unused | ⋯ between the pads (`qam` is the same button) |

How far you pull a trigger, and how the pads rumble, are set in `[ps4]` / `[sc2]` in `config.toml`, not in this file.

## Screens

Use these names after `switchState.`:

| Name | Screen |
|------|--------|
| `keyboard` | on-screen keyboard |
| `menu` | menu |
| `textInput` | single-line field that intercepts typing |
| `moveWindow` | move and snap the overlay |
| `selectLayout` | pick a keyboard layout |
| `mappings` | edit bindings in the overlay |
| `selectKey` | pick a key inside that editor |

`[Keyboard]`, `[Menu]`, `[TextInput]`, `[MoveWindow]`, and `[SelectLayout]` are the tables you can fill in `mappings.toml`. The mappings editor and key picker do not have their own tables.

An action that does not belong to that screen is ignored. A `when` string that does not parse stops that screen’s map from loading.

## `when` conditions

These names are true or false while you hold the button. You can combine them with `&&`, `||`, `!`, and parentheses.

| Name | True when |
|------|-----------|
| `suggestionSelected` | a word-suggestion chip is highlighted |
| `completionActive` | word suggestions are on |
| `justAccepted` | a chip was just accepted and nothing has been typed since |
| `modifier` | Shift, Ctrl, or Alt is sticky-on |
| `modifier.shift` | Shift is sticky-on (`shift` is the same flag) |
| `modifier.ctrl` | Ctrl is sticky-on (`ctrl` is the same flag) |
| `modifier.alt` | Alt is sticky-on (`alt` is the same flag) |
| `recording` | a tape is being recorded |
| `replay` | a tape is playing |

## Keyboard actions

| Name | What it does |
|------|----------------|
| `sendKeyUnderLeftStick` | type the key under the left stick |
| `sendKeyUnderRightStick` | type the key under the right stick |
| `sendKey.a` | type that character (`sendKey.A` is the same letter) |
| `sendKey.space` `sendKey.enter` `sendKey.tab` | space, Enter, Tab |
| `sendKey.backspace` `sendKey.delete` | Backspace, Delete |
| `sendKey.left` `sendKey.right` `sendKey.up` `sendKey.down` | arrow keys |
| `toggleShift` `toggleCtrl` `toggleAlt` | sticky modifiers |
| `paste` | paste |
| `switchState.menu` | go to that screen (any name from Screens) |
| `switchLayout.main` | switch to that layout name from `config.toml` |
| `flipWindowLeftRight` | flip the overlay left/right around the pointer |
| `flipWindowAboveBelow` | flip the overlay above/below the pointer |
| `rotateWindow` | cycle those flips |
| `exit` | quit kosk |
| `toggleRecord` | start or stop a `.krec` tape |
| `cycleSuggestion` | highlight the next suggestion chip |
| `cycleSuggestionPrev` | highlight the previous chip |
| `cancelSuggestion` | clear the chip highlight |
| `toggleCompletion` | turn word suggestions on or off |
| `acceptSuggestion` | type the highlighted chip |
| `acceptSuggestion.0` | type that chip by index, highlighted or not |

## Menu actions

`selectUp`, `selectDown`, `activate`, `switchState.…`

## Text input actions

`moveCursorLeft`, `moveCursorRight`, `submit` (accept the line), plus the same suggestion actions as the keyboard (`cycleSuggestion`, `cycleSuggestionPrev`, `cancelSuggestion`, `toggleCompletion`, `acceptSuggestion`, `acceptSuggestion.0`), and `switchState.…`

## Move-window actions

`nudgeUp`, `nudgeDown`, `nudgeLeft`, `nudgeRight`, `snapTopLeft`, `snapTopRight`, `snapBottomLeft`, `snapBottomRight`, `flipWindowLeftRight`, `flipWindowAboveBelow`, `rotateWindow`, `switchState.…`

## Select-layout actions

`selectUp`, `selectDown`, `activate`, `togglePreview`, `switchState.…`
