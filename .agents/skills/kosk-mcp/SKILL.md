---
name: kosk-mcp
description: Drive kosk via the Codex MCP server `kosk` — egui UI tools plus exclusive virtual controller hold-at. Use when inspecting/clicking kosk UI or injecting sticks/pads/buttons without a physical controller. Launch with EGUI_INSPECTION=1 and KOSK_CONTROLLER_MCP=1 (or --mcp-controller), attach to 127.0.0.1:5719, control axes on :5720, then disconnect and kill.
---

# kosk-mcp for kosk

## When to use

Use this skill to observe or drive live kosk through Codex:

- UI: query_tree / click / type / screenshot
- Controller: hold sticks/pads, press/tap buttons (physical HID and replay are disabled in MCP-controller mode)

## Prerequisites

1. `eframe` feature `inspection` enabled
2. Project MCP config: `.cursor/mcp.json` → server name `kosk`, command `cargo run -q -p kosk-mcp`
3. Workspace builds `kosk-mcp`

If the `kosk` MCP tools are missing, reload the MCP server from Codex and verify that `kosk` is listed.

## End-to-end flow (agent-owned)

### 1. Launch kosk in exclusive MCP-controller mode

```powershell
$env:EGUI_INSPECTION = "1"
$env:KOSK_CONTROLLER_MCP = "1"
cargo run -p kosk -- <config.toml>
```

Or:

```powershell
$env:EGUI_INSPECTION = "1"
cargo run -p kosk -- --mcp-controller <config.toml>
```

- UI inspection: `127.0.0.1:5719`
- Virtual controller port: `127.0.0.1:5720`
- No HID / no replay for this process — a physical stick cannot interfere

### 2. Attach

1. MCP `attach` → host `127.0.0.1`, port `5719`
2. Optional `status` / `controller_status`
3. Drive UI + controller
4. `disconnect` when finished

### 3. Drive

UI tools (same as egui-mcp): `query_tree`, `click`, `type_text`, `screenshot`, …

Controller tools (latent hold-at until `release_*` / `controller_neutral`, or optional `duration_ms`):

| Tool | Purpose |
|------|---------|
| `set_stick` / `release_stick` | Latch / clear stick XY |
| `set_pad` / `release_pad` | Explicit touching + XY |
| `press_button` / `release_button` / `tap_button` | Buttons |
| `set_trigger` | Analog trigger |
| `get_controller_state` / `controller_status` | Observe |
| `controller_neutral` | Clear all inputs |
| `geometry_ready` / `list_keys` / `get_key` / `stick_for_key` | Keyboard stick targets (post-map −1..1) |

After attach, call `geometry_ready` (wait until Keyboard first frame) then `stick_for_key` before inventing stick coords.

### 4. Teardown

1. MCP `disconnect`
2. Kill the kosk process you started

No mid-run disarm — mode is launch-time only.

## Security

Prefer loopback. Do not bind `0.0.0.0` without intent — inspection and controller ports have no auth.

## Checklist

- [ ] Reload the `kosk` MCP server if tools are missing
- [ ] Launch: `EGUI_INSPECTION=1` + `KOSK_CONTROLLER_MCP=1` (or `--mcp-controller`)
- [ ] `attach` → `127.0.0.1:5719`
- [ ] `geometry_ready` → `stick_for_key` before inventing stick coords
- [ ] `set_stick` / `set_pad` / buttons / `query_tree` / `screenshot`
- [ ] `disconnect` + kill kosk
