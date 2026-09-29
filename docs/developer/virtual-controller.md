# Virtual controller (`virtual_ctl.rs`, `control_server.rs`, `geometry_snap.rs`)

This document describes scripted input: the exclusive virtual controller and the local control port that drives it. The hold state lives in `src/controller/virtual_ctl.rs`, the TCP command server in `src/controller/control_server.rs`, and the read-only geometry queries in `src/state/keyboard/geometry_snap.rs`. The trait these feed is in [controller.md](controller.md); the flag that enables the mode is in [config.md](config.md).

## What problem this solves

Physical controllers need hands, and replay tapes need a recording made beforehand. Scripted input exists for the third case: a program holding sticks, touching pads, and pressing buttons on KOSK's behalf — automated checks, demonstrations, or agent-driven exploration. The virtual controller is exclusive by design. While it is on, the process never opens HID and refuses replay, so there is exactly one source of input and no fight between a script and a thumb.

## Enabling it

Pass `--mcp-controller`, or set the `KOSK_CONTROLLER_MCP` environment variable to a truthy value. The variable also accepts a `host:port` bind address; anything else falls back to `127.0.0.1:5720` with a warning, and `0` or `false` means off. At startup the process spawns the control server on that address, and device discovery returns the virtual controller immediately without touching HID. Combining this mode with replay is a startup error, since both claim to be the input.

## Holds, not events

The virtual controller is level state, not an event stream. Setting a stick to `(0.8, 0.0)` holds it there across polls until it is released or an optional duration expires; the same applies to pads (with an explicit touching flag), buttons, and triggers. `neutral` clears everything at once. Each poll — every 4 milliseconds — snapshots the held state: an engaged snapshot reaches the app as controller input, and a fully released one reads as idle, exactly like a resting physical device. Sticks and pads report post-map coordinates in the same −1…1 space the keyboard consumes, so a script aims with the numbers the layout math expects rather than raw hardware units.

The wire protocol is newline-delimited JSON over TCP. The commands are small verbs: `ping`, `neutral`, `get_state`, `set_stick` and `release_stick`, `set_pad` and `release_pad`, `press`, `release`, and `tap` (a press with a default 50 ms expiry), and `set_trigger` with a 0–255 value or null. Every command carries an id and gets an ok or error reply. Buttons are named with the same vocabulary as mappings (`faceBottom`, `padLeft`, chords are not a concept here — each button is held independently).

## Geometry queries

Aiming blind would be guesswork, so the server also answers questions about the drawn keyboard. After the UI captures key centers, it publishes an immutable geometry snapshot: layout name and revision, scales, stick rest points, bound rectangles, and every key with its center and hitbox. Control threads read only this snapshot and never touch the live keyboard, which would be a locking hazard from a socket thread.

Four read-only commands expose it. `geometry_ready` reports whether a snapshot exists yet — scripts must wait for the first keyboard draw before asking anything else. `list_keys` returns each key's id, row, column, selectability, and whether it has a hitbox. `get_key` returns one key's center, hitbox, rest points, and scales. `stick_for_key` runs the mapping backwards: given a side and a key, it returns the stick coordinates that land on that key's center, whether the answer had to be clamped into −1…1, whether the key is actually reachable (with a reason when it is not, such as no hitbox or a clamped point that lands on a neighbor), and optionally the stick-space bounding box of the hitbox. When the center-derived point misses but the bounding-box center hits, the server prefers the point that hits.

## What this does not cover

This page does not cover HID discovery or the `ControllerInput` trait beyond the virtual variant; that is the controller document. It does not cover hitbox math itself; that is shared code described in [keyboard-layout.md](keyboard-layout.md). Recording still taps virtual input like any other source, so scripted runs can produce tapes — but that flow is described in [record-replay.md](record-replay.md).

## Summary

The virtual controller replaces the HID device with script-held state served over a local JSON port, polled like any controller and never mixed with physical or replay input. Holds persist until released or expired, and a read-only geometry snapshot lets scripts convert key names into the stick positions that reach them.
