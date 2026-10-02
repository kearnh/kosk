# Controller button glyphs (`controller_glyph.rs`)

This document describes how on-screen prompts draw a controller button. The mapping and the draw helper live in `src/ui/controller_glyph.rs`. The SVG files are embedded from `assets/controller-glyphs/kenney/`. Settings, the mappings screen, and move-window mode all call the same helper. Placement of the overlay is in [window-position.md](window-position.md). The settings footer is in [menu.md](menu.md).

## What problem does this solve?

A prompt should show the button the person is holding, in the art for the controller they are holding. `faceBottom` is A on a Steam Controller and Cross on a DualShock 4. Keyboard code, the settings footer, and move-window should not each pick a file path. They pass a `ControllerButton` and a `GlyphFamily`, and `controller_glyph::show` draws the icon.

The art comes from Kenney Input Prompts 1.5A under CC0. Source, license, and substitutions are recorded in [the glyph notice](../../assets/controller-glyphs/README.md). White icons need a dark background.

## `GlyphFamily`

`GlyphFamily` is `Sc2` or `Ps4`. `GlyphFamily::from_kind` chooses one from the controller snapshot’s `ControllerKind`:

- A live Steam Controller 2 uses `Sc2`.
- A live DualShock 4 uses `Ps4`.
- Replay, including the virtual controller, reports `ControllerKind::Replay`. The helper then walks `preferred_controller` and uses the first entry that is `Sc2` or `Ps4`. If that list has no such entry, it uses `Sc2`.

Discovery order uses the same config list, but that walk is separate. Glyph selection only reads it when the snapshot’s family is `Replay`. See [controller.md](controller.md) and [config.md](config.md).

## Which file is drawn

`svg_bytes` returns the embedded bytes for one family and one button. `show` hands those bytes to egui as an image whose URI ends in `.svg`, which is what the SVG loader requires. The bytes are compiled in with `include_bytes!`. Tests parse and render every button for both families. The gallery checks their appearance.

Face buttons follow the semantic names. `FaceBottom` is `steam_button_color_a.svg` for `Sc2` and `playstation_button_color_cross.svg` for `Ps4`. `FaceRight`, `FaceLeft`, and `FaceTop` follow the same split: B/X/Y on the Steam Controller, Circle/Square/Triangle on the DualShock 4.

Shoulders, triggers, the d-pad, Options, Share, System, pad clicks, and stick clicks differ by family. Steam Controller uses its view/menu icons and Steam symbol. DualShock 4 uses its Share/Options icons and a home symbol for System. Steam Controller square pad and stick-click prompts use Steam Deck artwork; DualShock 4 uses its own touchpad and stick-click artwork.

Rear buttons and Quick Access use Steam Controller artwork for both families. DualShock 4 does not have those buttons. The mapping still has art so a prompt can draw them if a binding names them.

## Where prompts call `show`

Each caller takes the current snapshot’s kind, converts it with `GlyphFamily::from_kind`, and draws only the buttons that binding mentions.

The settings footer in `src/state/menu.rs` draws each bound button at 16px, then a short word. A remapped button shows that button’s glyph. The mappings screen in `src/state/mappings.rs` uses the same 16px size for its hint row. Move-window mode in `src/state/move_window.rs` draws the Save and Cancel bindings at 28px beside those labels. The labels are words, not egui buttons.

## Checking the whole set

`cargo run --bin glyph_gallery` opens a dark window with one row per `ControllerButton`. The Sc2 column and the Ps4 column sit side by side, each icon at 32px. The binary calls `kosk::controller_glyph::show`. It does not open a controller and it does not read `preferred_controller`.

## What this does not cover

**Which HID bit becomes Share or Options is not decided here.** Steam Controller 2’s view and menu bits are described in [devices.md](devices.md). This page only chooses the picture after the button enum value exists.

**The gallery does not follow a connected controller.** The running app shows one family. The gallery always shows both, so a wrong file is visible without plugging in both devices.

**Letters on the on-screen keyboard are not these icons.** Key faces come from the layout and, for some keys, Phosphor icons. That drawing is in [keyboard.md](keyboard.md).

## Summary

`controller_glyph::show` draws one embedded CC0 SVG for a `ControllerButton` and a `GlyphFamily`. Rear buttons and Quick Access share files; other buttons use family-specific artwork. Replay picks a family from `preferred_controller`, or `Sc2` if that list does not name one. The glyph gallery binary is the visual check of every pair.
