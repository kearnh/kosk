# Controller button glyphs (`controller_glyph.rs`)

This document describes how on-screen prompts draw a controller button. The mapping and the draw helper live in `src/ui/controller_glyph.rs`. The SVG files are embedded from `assets/controller-glyphs/knockout/`. Settings, the mappings screen, and move-window mode all call the same helper. Placement of the overlay is in [window-position.md](window-position.md). The settings footer is in [menu.md](menu.md).

## What problem does this solve?

A prompt should show the button the person is holding, in the art for the controller they are holding. `faceBottom` is A on a Steam Controller and Cross on a DualShock 4. Keyboard code, the settings footer, and move-window should not each pick a file path. They pass a `ControllerButton` and a `GlyphFamily`, and `controller_glyph::show` draws the icon.

The art is Kenney Input Prompts 1.5A under CC0 1.0, embedded under legacy filenames. Source and release status are recorded in [the glyph notice](../../assets/controller-glyphs/README.md). Knockout icons that are a single white fill disappear on a white panel, so callers draw them on a dark background.

## `GlyphFamily`

`GlyphFamily` is `Sc2` or `Ps4`. `GlyphFamily::from_kind` chooses one from the controller snapshot’s `ControllerKind`:

- A live Steam Controller 2 uses `Sc2`.
- A live DualShock 4 uses `Ps4`.
- Replay, including the virtual controller, reports `ControllerKind::Replay`. The helper then walks `preferred_controller` and uses the first entry that is `Sc2` or `Ps4`. If that list has no such entry, it uses `Sc2`.

Discovery order uses the same config list, but that walk is separate. Glyph selection only reads it when the snapshot’s family is `Replay`. See [controller.md](controller.md) and [config.md](config.md).

## Which file is drawn

`svg_bytes` returns the embedded bytes for one family and one button. `show` hands those bytes to egui as an image whose URI ends in `.svg`, which is what the SVG loader requires. The bytes are compiled in with `include_bytes!`. A unit test checks that every `ControllerButton` for both families starts with `<svg`. It does not check that the picture is the right button.

Face buttons follow the semantic names, not a shared Xbox sheet for both devices. `FaceBottom` is `shared_color_button_a.svg` for `Sc2` and `ps_color_button_x.svg` for `Ps4`. `FaceRight`, `FaceLeft`, and `FaceTop` follow the same split: B/X/Y on the Steam Controller, Circle/Square/Triangle on the DualShock 4.

Shoulders, triggers, the d-pad, Options, Share, System, and the pad clicks also differ by family. On `Sc2`, Share is `sd_button_view.svg` and Options is `sd_button_menu.svg` (the Steam Deck view and menu icons). On `Ps4`, those are `ps4_button_share.svg` and `ps4_button_options.svg`. System uses a controller symbol on `Sc2` and a home symbol on `Ps4`.

Stick clicks, the rear paddles, and Quick Access use the same file for both families: `shared_l3.svg`, `shared_r3.svg`, `sc_l4.svg`, `sc_l5.svg`, `sc_r4.svg`, `sc_r5.svg`, and `qam_icon.svg`. DualShock 4 does not have paddles or Quick Access. The mapping still has art for those buttons so a prompt can draw them if a binding names them.

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

`controller_glyph::show` draws one embedded knockout SVG for a `ControllerButton` and a `GlyphFamily`. Steam Controller 2 and DualShock 4 use different face, shoulder, trigger, d-pad, and system art. Share and Options on the Steam Controller use the Deck view and menu icons. Stick clicks, paddles, and Quick Access share files. Replay picks a family from `preferred_controller`, or `Sc2` if that list does not name one. The glyph gallery binary is the visual check of every pair.
