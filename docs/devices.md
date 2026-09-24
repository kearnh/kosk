# Device drivers (`ps4.rs`, `sc2.rs`, `pad_origin.rs`)

This document describes the HID implementations behind `ControllerInput`: DualShock 4 in `src/controller/ps4.rs`, Steam Controller 2 in `src/controller/sc2.rs`, and the pad-origin mapping that only Steam Controller 2 uses in `src/controller/pad_origin.rs`. Discovery and the trait itself are in [controller.md](controller.md).

## DualShock 4

`Ps4Device::open` looks for Sony vendor `0x054c` and DualShock 4 product `0x09cc`. It reads HID reports in a loop with a timeout.

Two report IDs are accepted. `0x01` is the ordinary USB state report; stick and button bytes start at offset 1. `0x11` is the longer “Steam mode” report, where the same 9-byte state block starts at offset 3. Unknown report IDs are ignored.

Sticks in the report are 0–255 with 128 as center. The driver subtracts 128, applies a small dead zone (`STICK_THRESHOLD`), and scales to approximately −1…1. Y is inverted so that up on the stick is negative in the same convention the keyboard uses.

D-pad and face buttons are packed into the state bytes the usual DualShock way (hat nibble plus Cross/Circle/Square/Triangle bits). `face_bottom` is Cross, `face_right` is Circle, `face_top` is Triangle, `face_left` is Square. `btn_options`, `btn_share`, and `btn_system` are Options, Share, and PS.

Triggers become `Option<u8>` by comparing the analog byte to `config.ps4.trigger_left_threshold` / `trigger_right_threshold`. Below threshold is `None`; at or above is `Some(value)`. DualShock 4 has no pads, paddles, or Quick Access, so those trait methods stay on the defaults.

`is_engaged` is true when either stick is off center or any digital (including a trigger that passed threshold) is down. An idle poll still produces a snapshot in some paths; the iterator uses engagement to decide whether the app should see `None`.

There is no haptic output and no pad-origin processing on this device.

## Steam Controller 2

The Steam Controller 2 driver is a HID client for Valve’s Triton devices. Product IDs cover wired (`0x1302`), BLE (`0x1303`), and related IDs (`0x1304`, `0x1305`) under vendor `0x28de`. Report layout, button masks, and the “lizard mode” feature report follow SDL’s hidapi Steam Triton driver; the file is not a copy of those sources, only the constants and packing.

State reports use IDs `0x42`, `0x45` (BLE), and `0x47`. The parsed `Sc2State` holds a button bitfield, analog triggers, analog sticks, pad coordinates, and pad pressure. IMU is ignored.

### Pads as sticks

For the on-screen keyboard, `left_stick_raw` / `right_stick_raw` on `Sc2State` return the **touchpads**, not the physical analog sticks. `pad_as_stick` converts a 16-bit pad sample to −1…1 and inverts Y, and returns `(0, 0)` when that pad is not touched. Physical sticks remain in the struct (`physical_left_stick`) for debugging and for `sc2_test`; they do not move the keyboard highlight in v1.

Pad clicks are `pad_left` / `pad_right`. Rear paddles are `l4`, `l5`, `r4`, `r5`. The Quick Access (⋯) button between the pads is `quickAccess` (`BTN_QAM`). Face buttons follow the Xbox-style layout mapped onto KOSK’s semantic names (A is `face_bottom`, and so on). The Triton MENU bit is the left button beside Steam and fills Share (`view` is the same button); the VIEW bit is the right one and fills Options (`menu` is the same button). Steam fills System.

Triggers use both analog value and a click bit. `scale_trigger` reports `Some` when the click is down or the scaled analog value meets `[sc2]` threshold.

### Lizard mode and haptics

Without a periodic feature report, Steam Controller firmware can fall back to “lizard mode” (OS mouse/keyboard emulation). The driver refreshes that report about every three seconds so KOSK keeps exclusive-style input.

When a pad is clicked, the driver may send a one-shot haptic command (`0x82`) whose strength comes from `touchpad_left_haptic` / `touchpad_right_haptic` (`none`, `low`, `medium`, `high`). Failures are ignored; missing haptics must not drop input.

### Idle and the iterator

`ConnectedController::Sc2` does not yield raw `Sc2State` to the app. Each engaged poll runs `PadOriginMapper::map` and boxes the result as `MappedSc2Input`. On idle (`None`), the mapper is reset so the next touch captures a new origin.

## Pad-origin mapping

Pads map to the keyboard as a stick in −1…1. Two knobs in `[sc2]` control that mapping. Both are read every poll, so saving `config.toml` changes feel without a restart.

`pad_origin_relative` (0…1) chooses where stick zero sits. The mapper stores the first stable touch `T` after `pad_origin_settle_ms` (default 20 ms). Each frame the origin is `T * relative`:

- `0` — origin is pad center. Output is the absolute pad position (KOSK’s original mapping).
- `1` — origin is `T`. First contact is stick rest; further motion is relative to that touch.
- Values in between put origin on the line from `(0, 0)` to `T`, so contact lands part-way between rest and the absolute pad key.

Until settle finishes there is no frozen `T`. Output is `pos * (1 - relative)`, so a full-relative landing stays on rest during the contact spike, and an absolute landing still tracks the pad.

`pad_origin_stretch` (`k`, 0…1) stretches leftover travel on the short remaining edge from that origin so a short-side landing can still approach ±1. `k = 0` is 1:1 `pos - origin`. Extra gain applies only toward the short edge and is capped by `pad_origin_stretch_max_gain`. The long edge is not compressed. Axes are independent. Lift clears `T`.

`MappedSc2Input` stores those mapped coordinates and implements `left_pad` / `right_pad` as `warp(mapped, stick_warp)`. Raw pad methods still report the unstretched sample, which matters for recordings: snapshots taken through `ControllerInput::left_pad` store the **mapped and warped** values, so replay must not apply origin mapping or warp a second time.

The DualShock 4 path never constructs a `PadOriginMapper`.

## The `sc2_test` binary

`src/bin/sc2_test.rs` is a small HID console for checking reports without the overlay. It is not part of the GUI process. Use it when pad origin or lizard mode looks wrong and you want to see raw samples.

## What this does not cover

**Bindings do not live in the drivers.** `padRight` meaning “send the key under the right stick” is a `mappings.toml` line, not a HID default.

**Stick-to-key hit testing** is layout code. Drivers only produce −1…1 samples (after stretch/warp).

## Summary

DualShock 4 is a straightforward HID state report with analog sticks and thresholded triggers. Steam Controller 2 is a Triton HID client that treats pads as the keyboard sticks, keeps lizard mode suppressed, optionally ticks haptics on pad click, and maps first-touch origin (blended toward pad center) plus short-edge stretch before the shared circle-to-square warp. Replay stores the post-map stick values, so the mapper is a live-device concern only.
