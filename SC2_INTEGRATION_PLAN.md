# Steam Controller 2 (Triton) integration plan

Draft plan for adding Steam Controller 2 support to kosk, with trackpads driving the same stick-based keyboard selection path that DualShock 4 sticks use today.

**Status:** planning only — no implementation yet.  
**Primary goal:** SC2 left/right trackpads → `left_stick` / `right_stick` semantics for keyboard key selection.  
**Secondary goal:** device discovery beyond hardcoded PS4 VID/PID.  
**Remapping / profiles:** deferred — see [CONTROLLER_REMAPPING_PLAN.md](./CONTROLLER_REMAPPING_PLAN.md).

---

## 1. Current kosk architecture (relevant bits)

kosk is a **single-device, DualShock 4–only** HID stack:

```
main.rs (background thread)
  HidApi → find_device(0x054c, 0x09cc)  [poll 500ms]
  Ps4Device::new(HidDevice) → Iterator<Item = Option<Box<dyn ControllerInput>>>
       │
       ▼
AppState::handle_controller_input
  Keyboard: layout nearest-key from input.left_stick() / right_stick()
            + BindingEngine for digital buttons
  Menu / MoveWindow / TextInput: bindings (sticks only matter when TextInput
  forwards to keyboard)
```

Key files:

| Path | Role |
|------|------|
| `src/controller/mod.rs` | `ControllerInput`, `ControllerButton`, stick warp |
| `src/controller/ps4.rs` | DS4 HID parse + `Ps4Device` iterator |
| `src/controller/bindings.rs` | `BindingEngine` (edges, chords, while-held) |
| `src/main.rs` | Discovery + reconnect loop (hardcoded PS4) |
| `src/state/keyboard/` | Stick → cursor → nearest key |

Important facts:

- **Sticks are not binding keys.** Axes go straight into the keyboard layout. Only L3/R3 (`stick_left` / `stick_right`) are digital bindings.
- **`ControllerInput` is already semantic** (`face_bottom`, not “Cross”). That is the right stable surface for cross-device work.
- **There is no device trait** — only `Ps4Device` produces snapshots. Discovery is “first openable `054c:09cc`”.
- **Deps:** `hidapi` only. No SDL / gilrs.
- `old_steam_controller_kb.toml` is a **keyboard layout** for pad-style typing, not an SC HID driver.

---

## 2. What SC2 is at the HID layer

Codename **Triton** / **Ibex**. Valve VID `0x28de`. Relevant PIDs (from SDL + community captures):

| PID | Device |
|-----|--------|
| `0x1302` | Wired Steam Controller 2 |
| `0x1303` | BLE Steam Controller 2 |
| `0x1304` | Proteus USB puck (dongle) — up to 4 controller slots |
| `0x1305` | Nereid dongle (same driver path as Proteus in SDL) |

**Recommendation for kosk:** stay on **raw HID via `hidapi`**, crib report layout and lizard-mode handling from SDL’s `SDL_hidapi_steam_triton.c`. Pulling in SDL3 just for one device is heavy relative to the existing DS4 path; the Triton protocol is small and well documented in open source.

### 2.1 Puck USB topology (wireless)

The Proteus puck (`28de:1304`) is a composite device. Controller state lives on **HID interfaces 2–5** (slots 0–3). Interface 6 is a puck status channel. With one paired controller, only slot 0 streams state; open the correct interface, not “any” Valve HID node.

Wired `0x1302` is a single controller path (SDL treats non-dongle Triton as immediately connected).

### 2.2 Input report IDs (multiplexed on one endpoint)

| Report ID | Size (incl. ID) | Purpose |
|----------:|----------------:|---------|
| `0x40` | 6 B | Lizard-mode mouse |
| `0x41` | 9 B | Lizard-mode keyboard |
| **`0x42`** | **54 B** | **Primary controller state** (`ID_TRITON_CONTROLLER_STATE`) |
| `0x43` | 15 B | Battery |
| `0x45` | 46 B | BLE state variant |
| `0x46` / `0x79` | 2 B | Wireless connect/disconnect |
| **`0x47`** | ~same + pad timestamp | Newer firmware state (`ID_TRITON_CONTROLLER_STATE_TIMESTAMP`) |
| `0x7b` | 13 B | Puck status (not used by SDL; ignore for v1) |

kosk must **filter by report ID**. Blindly treating every read as state will break on lizard / battery / wireless packets.

### 2.3 Report `0x42` / `0x45` payload (`TritonMTUNoQuat_t` / Full)

After the 1-byte report ID, packed little-endian (from SDL `controller_structs.h`, verified by [sc2-research](https://github.com/CouchTurtle/sc2-research)):

| Offset | Type | Field |
|-------:|------|-------|
| +0 | `u8` | `seq_num` |
| +1 | `u32` | `buttons` bitmask |
| +5 | `i16` | `sTriggerLeft` |
| +7 | `i16` | `sTriggerRight` |
| +9 | `i16` | `sLeftStickX` |
| +11 | `i16` | `sLeftStickY` |
| +13 | `i16` | `sRightStickX` |
| +15 | `i16` | `sRightStickY` |
| +17 | `i16` | `sLeftPadX` |
| +19 | `i16` | `sLeftPadY` |
| +21 | `u16` | `unPressureLeft` |
| +23 | `i16` | `sRightPadX` |
| +25 | `i16` | `sRightPadY` |
| +27 | `u16` | `unPressureRight` |
| +29… | IMU | timestamp + accel/gyro (+ quat on Full) |

`0x47` inserts `unTrackpadTimestamp` before the left pad and uses a 16-bit IMU timestamp (×32 µs). Parse path should accept both `0x42`/`0x45` and `0x47` (identical through sticks; pads share the same normalization).

**Frame rate:** ~250–266 Hz (~4 ms). Fine for the existing “read until idle” iterator model.

### 2.4 Button bitmask (`TritonButtons`)

From SDL (same bits used for touchpad touch/click):

| Mask | Meaning |
|------|---------|
| `0x00000001` | A |
| `0x00000002` | B |
| `0x00000004` | X |
| `0x00000008` | Y |
| `0x00000010` | QAM |
| `0x00000020` | R3 |
| `0x00000040` | View |
| `0x00000080` | R4 (paddle) |
| `0x00000100` | R5 (paddle) |
| `0x00000200` | RB |
| `0x00000400` | D-pad Down |
| `0x00000800` | D-pad Right |
| `0x00001000` | D-pad Left |
| `0x00002000` | D-pad Up |
| `0x00004000` | Menu |
| `0x00008000` | L3 |
| `0x00010000` | Steam |
| `0x00020000` | L4 (paddle) |
| `0x00040000` | L5 (paddle) |
| `0x00080000` | LB |
| `0x00100000` | Right stick capacitive touch |
| `0x00200000` | **Right trackpad touch** |
| `0x00400000` | **Right trackpad click** |
| `0x00800000` | Right trigger click |
| `0x01000000` | Left stick capacitive touch |
| `0x02000000` | **Left trackpad touch** |
| `0x04000000` | **Left trackpad click** |
| `0x08000000` | Left trigger click |
| `0x10000000` | Right grip touch |
| `0x20000000` | Left grip touch |

Suggested first-pass mapping into kosk `ControllerInput`:

| Triton | kosk |
|--------|------|
| A / B / X / Y | `face_bottom` / `face_right` / `face_left` / `face_top` (Xbox-style: A=bottom, B=right, X=left, Y=top) |
| LB / RB | `shoulder_*` |
| L3 / R3 | `stick_left` / `stick_right` |
| D-pad | `dpad_*` |
| View / Menu / Steam | `btn_share` / `btn_options` / `btn_system` (exact names may need a tweak — DS4 Share/Options/PS vs Steam View/Menu/Steam) |
| Triggers | analog `i16` → `Option<u8>` (see below) |
| Trackpad click / paddles / QAM / grip | **not** in `ControllerButton` today — ignore for v1, or hardcode pad clicks if needed for “send key under stick” |

### 2.5 Trackpads → stick axes (the product goal)

SDL normalizes pad position for its touchpad API as:

```text
x = sPadX / 65536.0 + 0.5    // → ~[0, 1]
y = -sPadY / 65536.0 + 0.5   // Y flipped (PR #15528)
pressure = unPressure / 32768.0
```

kosk sticks want roughly **`[-1, 1]`** after `/128`-style scaling on DS4. For SC2 pads-as-sticks:

```text
if LEFT_TOUCHPAD_TOUCH:
  left_stick_raw = (sLeftPadX / 32767.0,  -sLeftPadY / 32767.0)  // clamp
else:
  left_stick_raw = (0, 0)
// same for right pad → right_stick_raw
```

Then existing `left_stick()` / `right_stick()` warp + keyboard layout apply unchanged.

**V1 policy (hardcoded, no remapping layer):**

1. **Touch-gated** — gating on touch matches “finger on pad = stick deflection” and avoids idle noise; release → `(0,0)` → idle edge / clear selection (same as DS4 stick return).
2. **Pads override sticks** — physical sticks remain in the report but are **ignored** for `*_stick_raw` in v1. Choosing sticks instead of pads is a later remapping concern ([CONTROLLER_REMAPPING_PLAN.md](./CONTROLLER_REMAPPING_PLAN.md)).
3. **Pad click (optional)** — if useful, hardcode left/right trackpad click toward “send key under stick” (or document a recommended `mappings.toml` once clicks exist as `ControllerButton`s). Prefer dedicated click bits over pressure.
4. **Deadzone / scale** — start with existing `stick_warp` / `stick_scale_*`, tune after hardware testing.

### 2.6 Triggers

Report carries full `i16` hall triggers plus digital click bits. kosk’s `trigger_*() -> Option<u8>` currently means “digital pressed → Some(analog)”. For Triton:

- Scale `sTrigger*` from `0..32767` (or whatever idle/full range is measured) to `u8`.
- Return `Some` when value ≥ threshold **or** when `*_TRIGGER_CLICK` is set (so pure-analog pull still works).

### 2.7 Lizard mode (required)

Without Steam (or SDL) holding lizard off, the controller emits keyboard/mouse (`0x40`/`0x41`) and can steal desktop focus — bad for an OSK.

SDL disables lizard via **HID feature report** (`ID_SET_SETTINGS_VALUES` = `0x87`):

```text
SETTING_LIZARD_MODE = LIZARD_MODE_OFF
```

and **re-sends every ~3 s** (hardware watchdog re-enables lizard in ~8 s if not refreshed).

kosk’s SC2 device loop must:

1. On open: send lizard-off feature report.
2. While connected: refresh on a timer (~3 s).
3. On close: do nothing special (watchdog restores lizard) — same as SDL.

Feature report framing (SDL): 64-byte buffer, byte0 = `1` (report ID quirk for hidapi), then `FeatureReportMsg` header + settings payload. Port carefully from `DisableSteamTritonLizardMode`.

### 2.8 IMU / haptics / Steam Input interference

- **IMU off by default** — enable only if we add gyro features later.
- **Haptics** — output reports `0x80+`; out of scope for v1.
- **Steam client** — with Steam running, Steam Input may also claim the device / inject desktop profile. Test with Steam quit for kosk development; document “close Steam or disable Steam Input for kosk” as a known operational note. Same issue SDL tracks separately from the HID driver.

### 2.9 Canonical SDL / research references

- Driver: [`SDL_hidapi_steam_triton.c`](https://github.com/libsdl-org/SDL/blob/main/src/joystick/hidapi/SDL_hidapi_steam_triton.c)
- Structs: [`controller_structs.h`](https://github.com/libsdl-org/SDL/blob/main/src/joystick/hidapi/steam/controller_structs.h) (`TritonMTU*`, report IDs)
- Touchpad extras: [SDL PR #15528](https://github.com/libsdl-org/SDL/pull/15528) (merged; pads already in core driver)
- Empirics: [CouchTurtle/sc2-research `HID_REPORT_FORMAT.md`](https://github.com/CouchTurtle/sc2-research/blob/main/docs/HID_REPORT_FORMAT.md)

---

## 3. Device discovery / connection (beyond “always Ps4Device”)

Today `main.rs` only looks for DS4. Needed shape:

### 3.1 Prefer a small `ControllerDevice` producer trait

```rust
trait ControllerDevice: Iterator<Item = Option<Box<dyn ControllerInput>>> {
    // optional: fn device_kind(&self) -> DeviceKind;
}
```

`Ps4Device` and `Sc2Device` both implement this. `main` picks one and iterates — same reconnect outer loop.

### 3.2 Enumeration strategy (v1)

Priority allowlist, first successful open wins (still single-device — enough for now):

1. Valve puck `28de:1304` / `1305`, HID iface 2–5 with live `0x42` traffic (or first openable slot).
2. Wired Triton `28de:1302` (and BLE `1303` if present as HID).
3. Sony DS4 `054c:09cc` (existing).

Config override later: `preferred_controller = "sc2" | "ps4" | "auto"`.

### 3.3 Explicit non-goals for v1

- Multi-controller merge / hot-swap between two active pads.
- Steam Input API / XInput virtual pads.
- Full paddle / grip / QAM exposure in `mappings.toml`.
- Configurable remapping / profiles (see [CONTROLLER_REMAPPING_PLAN.md](./CONTROLLER_REMAPPING_PLAN.md)).

---

## 4. Remapping (out of scope for this plan)

V1 hardcodes pads-as-sticks in the SC2 → `ControllerInput` adapter. No `MappedControllerInput`, no profiles, no axis-source config.

Deferred design (including why a generic `MappedControllerInput<T>` is a poor fit): **[CONTROLLER_REMAPPING_PLAN.md](./CONTROLLER_REMAPPING_PLAN.md)**.

---

## 5. Proposed implementation phases

### Phase 0 — Spike / capture (half day with hardware)

- Enumerate HID devices; note VID/PID/interface for the user’s setup (wired vs puck).
- Dump a few `0x42` frames while moving pads/sticks; confirm endianness and touch bits.
- Confirm lizard-off feature report works on Windows with this crate’s `hidapi`.

### Phase 1 — Device plumbing

- Add `src/controller/sc2.rs` (or `steam_triton.rs`): parse `0x42`/`0x47`, lizard refresh, iterator matching `Ps4Device`.
- Refactor discovery in `main.rs`: try SC2 then PS4 (or config).
- Map pads → `left_stick_raw` / `right_stick_raw`; buttons/triggers into existing trait.
- Manual test: keyboard selection with pads; idle release clears selection.

### Phase 2 — Bindings polish

- Decide Share/Options/Steam naming for SC2.
- Map trackpad clicks to “send key under stick” if needed (or document recommended `mappings.toml`).
- Paddles / QAM / grip: skip unless trivial.

### Phase 3 — Nice-to-haves

- Battery report, connection LED, haptics on key press.
- Prefer-open by serial / last-used device.
- True multi-device (unlikely for OSK).
- Remapping / profiles: [CONTROLLER_REMAPPING_PLAN.md](./CONTROLLER_REMAPPING_PLAN.md).

---

## 6. Risks / open questions

1. **Windows hidapi + Valve composite puck** — interface selection may need usage-page filtering; spike early.
2. **Steam running** — may fight for exclusive access or inject lizard/desktop profile.
3. **Face button layout** — Xbox-style A/B/X/Y vs PlayStation diamond; existing maps assume DS4 Cross=bottom. SC2 should use Xbox semantics or existing TOML will feel wrong.
4. **Trigger scale** — measure real min/max before hardcoding `/256`.
5. **Pad Y orientation** — follow SDL’s flip; verify against keyboard layout feel.
6. **License / attribution** — SDL code is zlib; cribing constants/layout is fine with attribution in comments; do not copy large SDL files wholesale if avoidable.

---

## 7. Verdict / recommendation

| Topic | Recommendation |
|-------|----------------|
| I/O stack | Stay on `hidapi`; port Triton report + lizard logic from SDL (do not add SDL dep for v1). |
| Pads as sticks | Hardcode in SC2 adapter: touch-gated pad XY → `*_stick_raw`; ignore physical sticks; leave keyboard unchanged. |
| Discovery | Allowlist VID/PID (+ puck iface); first match; shared iterator contract. |
| Remapping / profiles | Out of scope — [CONTROLLER_REMAPPING_PLAN.md](./CONTROLLER_REMAPPING_PLAN.md). |
| Action bindings | Keep existing semantic `controller_map` / `BindingEngine`. |

This gets SC2 trackpad typing working with minimal churn.
