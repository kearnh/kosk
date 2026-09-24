# Controller abstraction (`controller/mod.rs`)

This document describes the device-independent layer in `src/controller/mod.rs`: the `ControllerInput` trait, button and chord names used in mappings, stick warp, and how `find_device` chooses a HID family. Concrete DualShock 4 and Steam Controller 2 parsing is in [devices.md](devices.md). How mappings fire actions is in [bindings.md](bindings.md). Recording is in [record-replay.md](record-replay.md).

## What problem does this solve?

KOSK should not care, in keyboard code, whether a left-stick sample came from a DualShock analog stick or from a Steam Controller pad. Every device, including a replay tape, implements `ControllerInput`. The rest of the program reads sticks as `(f32, f32)` in roughly −1…1 and buttons as booleans (triggers as `Option<u8>` so “not pulled” is distinct from “pulled a little”).

The same module is also the parser for mapping keys. `mappings.toml` uses strings such as `faceTop` and `share + faceTop`. Those strings have to mean the same button on every device family.

## `ControllerInput`

The trait is a snapshot. One value is “this poll.” Implementations are `Debug` and cloneable through `box_clone`, because the debug plugin and the recording session need to keep a copy.

Raw sticks are `left_stick_raw` / `right_stick_raw`. The default `left_stick` / `right_stick` methods apply `warp(..., config.stick_warp)` and then clamp to the square. Steam Controller 2’s mapped input overrides `left_pad` / `right_pad` so pad-origin mapping runs *before* warp (see [devices.md](devices.md)). Replay overrides them too: a tape already stored warped (and, for SC2, origin-mapped) coordinates, so playing them back must not warp again.

Digital controls are one method per button. Names are semantic rather than PlayStation- or Steam-specific: `face_bottom` is Cross on a DualShock 4 and A on a Steam Controller. Pads (`pad_left`, `pad_right`), paddles (`l4`, `l5`, `r4`, `r5`), and Quick Access (`btn_quick_access`) default to false so DualShock 4 does not have to mention them.

`trigger_left` / `trigger_right` return `Some(value)` when the analog trigger is considered down (device threshold applied in the driver) and `None` when it is not. Bindings treat “some” as held; they do not use the analog value.

`is_engaged` is the idle test. A device that is completely at rest can yield `None` from the iterator so the app can reset binding edge state. The exact definition is per device (sticks moved, any digital down, and so on).

`family` returns `ControllerKind` for this snapshot (SC2, DualShock 4, or replay). Glyph drawing and similar UI use that to pick art. Replay and the virtual controller report `Replay`; the glyph layer then uses `preferred_controller`.

## Stick warp

Physical analog sticks move in a circle. The on-screen keyboard is a rectangle of keys. `warp` pushes samples toward the square corners as `stick_warp` goes from 0 (leave the circle) to 1 (fill the square). It then clamps to −1…1.

That function is applied on every `left_stick` / `right_stick` read unless the implementation bypasses the default. Keyboard hit-testing uses the warped coordinates, not the raw circle.

## Buttons and bindings as strings

`ControllerButton` is the unit used in mappings: dpad, face, shoulders, stick clicks, triggers, Options/Share/System, pads, L4/L5/R4/R5, and SC2 Quick Access (`quickAccess` / `qam`). Pads, paddles, and Quick Access default to not held on DualShock 4. `FromStr` is case-insensitive and ignores `-` and `_`, so `stick-left`, `stick_left`, and `stickLeft` are the same button. Arguments on a button (the old `triggerLeft,threshold=40` form) are rejected; thresholds belong in `[ps4]` / `[sc2]`.

`ControllerBinding` is either one button or a two-button chord `leader + follower`. Chord strings also parse through `FromStr`. Leader and follower must differ. These types serialize as the canonical display strings (`faceTop`, `share + faceTop`) so they can be TOML table keys. `view` parses as `share` and `menu` as `options`; both spellings load, and files are saved with the canonical names.

`ControllerButton::query` reads the corresponding `ControllerInput` method. Triggers query `is_some()` on the analog optional.

## Discovery

`ControllerKind` is `sc2`, `ps4`, or `replay`. `DEFAULT_CONTROLLER_ORDER` is Steam Controller 2 then DualShock 4. `resolve_controller_order` puts the user’s `preferred_controller` list first (deduplicated) and appends any built-in family that was omitted. Replay is never opened by that fill-in path.

`find_device` is what the controller thread calls:

1. If `--replay` was passed, or `preferred_controller` starts with `replay`, open `ReplayDevice` and return. HID is not touched.
2. Otherwise create `HidApi`, walk the resolved order, and return the first family that opens.

`ConnectedController` is the enum the thread iterates. Each `next()` is one poll: `Some(Some(snapshot))` when engaged, `Some(None)` when idle, `None` when the device is gone. Steam Controller 2’s variant also holds a `PadOriginMapper` that is reset on idle so the next touch is a new origin.

## What this does not cover

**This module does not fire actions.** It only names buttons and produces snapshots. `BindingEngine` consumes those snapshots.

**Warp is not stick-to-key mapping.** After warp, layout code scales the sample by `stick_scale_*` and looks up a hitbox. That is [keyboard-layout.md](keyboard-layout.md).

**Haptics, lizard mode, and report IDs** are inside the device files.

## Summary

`controller/mod.rs` is the vocabulary the rest of KOSK uses for “a controller.” Everything reports sticks and buttons through `ControllerInput`. Mapping files talk about those buttons with a small string language. `find_device` picks replay, Steam Controller 2, or DualShock 4 according to config, and the controller thread iterates whatever it opened until the device disappears.
