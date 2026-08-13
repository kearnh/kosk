# Controller remapping / profiles plan

**Status:** deferred — do after SC2 v1 works.  
**Depends on:** [SC2_INTEGRATION_PLAN.md](./SC2_INTEGRATION_PLAN.md) v1 (device open + pads hardcoded as sticks).  
**Not in scope for SC2 v1:** configurable axis sources, button swaps, named profiles, or `MappedControllerInput`.

This document captures the remapping design discussion so it can be picked up later without blocking SC2 support.

---

## Motivation

Today kosk has two layers that are easy to confuse:

1. **Hardware → `ControllerInput`** — hardcoded per device (`Ps4InputData` today; SC2 v1 will hardcode pads → stick axes in the SC2 adapter).
2. **`ControllerInput` → actions** — `BindingEngine` + TOML `controller_map` (semantic names like `faceTop`).

Layer (2) already works and should stay. Remapping here means making layer (1) configurable: e.g. “SC2 physical sticks instead of pads for keyboard”, “swap A/B”, or sharing one action map across differently laid-out hardware without editing every binding.

---

## Rejected / problematic approach

### `MappedControllerInput<T: ControllerInput>: ControllerInput`

Idea: generic decorator that remaps an inner `ControllerInput` and itself implements `ControllerInput`.

| Issue | Why it hurts |
|-------|----------------|
| **Object safety / dyn** | App uses `Box<dyn ControllerInput>`. Wrapping `T` is fine statically; wrapping `dyn` forces `MappedControllerInput<Box<dyn ControllerInput>>` or double erasure. |
| **Axes aren’t in `ControllerButton`** | Button-only maps cannot express “left pad feeds left stick”. |
| **Source fields differ by device** | SC2 has pads, sticks, pad click, grip touch. DS4 has none of those. Mapping from an already-semantic `ControllerInput` loses native fields unless the trait is widened first. |
| **Double warp risk** | Default `left_stick()` applies `stick_warp`. Remapping after warp (or remapping then warping again) is easy to get wrong. Prefer remapping **raw** axes only. |
| **Overlap with action maps** | Users already edit `faceTop` → actions. A second TOML that remaps `faceTop` → `faceBottom` invites “which map?” confusion. |

Do **not** build this generic wrapper as the first remapping design.

---

## Preferred design

Split stages explicitly:

```text
┌─────────────────────┐
│ Device-native report│  Ps4Report / TritonReport (concrete structs)
└──────────┬──────────┘
           │  Device adapter (per hardware family; SC2 v1 hardcodes pads→sticks here)
           ▼
┌─────────────────────┐
│ ControllerInput     │  semantic snapshot (existing trait)
└──────────┬──────────┘
           │  optional RemappedInput (this plan — later)
           ▼
┌─────────────────────┐
│ ControllerInput     │  still semantic; axes/buttons possibly swapped
└──────────┬──────────┘
           │  BindingEngine (existing controller_map)
           ▼
        Actions
```

### Stage A — hardware adapters (partially done by SC2 v1)

Each device family parses its report and produces `ControllerInput`. SC2 v1 hardcodes:

- trackpads (touch-gated) → `left_stick_raw` / `right_stick_raw`
- face buttons Xbox-style into `face_*`
- etc.

No profile system required yet.

### Stage B — optional semantic remapper (this task)

When a second remapping need appears (e.g. “I want physical sticks for keyboard on SC2”):

```rust
enum AxisSource {
    LeftStick,
    RightStick,
    // only meaningful if ControllerInput (or a richer snapshot) exposes pads:
    LeftPad,
    RightPad,
    // …
}

struct InputRemap {
    left_axis: AxisSource,
    right_axis: AxisSource,
    // button swaps: FaceBottom -> FaceRight, …
}

struct RemappedInput {
    inner: Box<dyn ControllerInput>, // or owned SemanticSnapshot
    remap: Arc<InputRemap>,
}

impl ControllerInput for RemappedInput {
    // forward with axis source selection + button swaps
}
```

Guidelines:

- Prefer **owned snapshot + remap** (or `Box<dyn ControllerInput>`), not `MappedControllerInput<T>`, for `dyn`-friendliness.
- Express **axis sources** explicitly (`AxisSource`), not only button remaps.
- Keep remap tables **per device kind / profile name**, separate from `controller_map` action bindings.
- Apply remaps to **raw** stick/pad values; let `left_stick()` / `right_stick()` warp once on the outer type.

### Extending `ControllerInput` (likely needed for Stage B)

For first-class pads without forever stealing stick slots:

```rust
fn left_pad_raw(&self) -> Option<(f32, f32)>;  // None if no touch / unsupported
fn right_pad_raw(&self) -> Option<(f32, f32)>;
```

Default methods return `None`. Remapper (or keyboard) can choose pads if present else sticks. Also consider digital bindings: `PadClickLeft`, `PadClickRight`, `Paddle*` — only when mappings need them.

---

## Suggested profiles (when implementing)

| Profile name | Behavior |
|--------------|----------|
| `ps4` | Identity (current DS4 behavior) |
| `sc2_pads_as_sticks` | Same as SC2 v1 hardcoded default |
| `sc2_sticks` | Physical sticks → stick axes; pads unused or separate |

Config sketch (not committed):

```toml
[controller]
profile = "sc2_pads_as_sticks"
# or preferred_controller = "auto" plus profile per kind
```

---

## Implementation phases (later)

1. **Inventory needs** — list concrete remaps users want beyond SC2 v1 defaults (don’t build abstract machinery first).
2. **Widen snapshot if needed** — `left_pad_raw` / paddles / etc. on `ControllerInput` or a parallel native struct held until remap.
3. **`InputRemap` + `RemappedInput`** — axis sources + button swaps; unit tests with fake snapshots.
4. **Named profiles in config** — load per `DeviceKind`; keep action `controller_map` unchanged.
5. **Docs** — clarify “hardware profile” vs “action bindings” in README / config comments.

---

## Out of scope (still)

- Mixing remapping into `controller_map` TOML keys.
- Steam Input / SDL gamepad mapping strings as the source of truth.
- Multi-controller simultaneous input merge.
