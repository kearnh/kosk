//! Loopback NDJSON control server for kosk-mcp virtual controller injection.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::str::FromStr;
use std::thread;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::controller::virtual_ctl::{session, StickSide};
use crate::controller::ControllerButton;
use crate::state::keyboard::geometry_snap::{self, GeometrySnapshot, SnapHitBox, StickAabb};

/// Spawn the accept loop on a dedicated thread. Fails if bind fails.
pub fn spawn(addr: SocketAddr) -> Result<()> {
    let listener =
        TcpListener::bind(addr).with_context(|| format!("bind kosk controller MCP port {addr}"))?;
    eprintln!("kosk controller MCP listening on {addr}");
    thread::spawn(move || {
        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    thread::spawn(move || {
                        if let Err(e) = handle_connection(stream) {
                            eprintln!("kosk controller MCP connection error: {e:#}");
                        }
                    });
                }
                Err(e) => eprintln!("kosk controller MCP accept error: {e}"),
            }
        }
    });
    Ok(())
}

fn handle_connection(stream: TcpStream) -> Result<()> {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "?".into());
    let mut reader = BufReader::new(stream.try_clone().context("clone tcp stream")?);
    let mut writer = stream;
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let resp = json!({"id": null, "ok": false, "error": format!("invalid json: {e}")});
                write_resp(&mut writer, &resp)?;
                continue;
            }
        };
        let resp = handle_command(req);
        write_resp(&mut writer, &resp)?;
    }
    let _ = peer;
    Ok(())
}

fn write_resp(writer: &mut TcpStream, resp: &Value) -> Result<()> {
    let mut s = serde_json::to_string(resp)?;
    s.push('\n');
    writer.write_all(s.as_bytes())?;
    writer.flush()?;
    Ok(())
}

fn id_of(req: &Value) -> Value {
    req.get("id").cloned().unwrap_or(Value::Null)
}

fn err(id: Value, msg: impl Into<String>) -> Value {
    json!({"id": id, "ok": false, "error": msg.into()})
}

fn err_msg(id: Value, error: &str, message: &str) -> Value {
    json!({"id": id, "ok": false, "error": error, "message": message})
}

fn ok(id: Value, extra: Value) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("id".into(), id);
    map.insert("ok".into(), Value::Bool(true));
    if let Value::Object(obj) = extra {
        for (k, v) in obj {
            map.insert(k, v);
        }
    }
    Value::Object(map)
}

/// Pure command dispatcher (testable without a live bind).
pub fn handle_command(req: Value) -> Value {
    let id = id_of(&req);
    let Some(cmd) = req.get("cmd").and_then(|c| c.as_str()) else {
        return err(id, "missing string field `cmd`");
    };

    // Geometry queries read the published snapshot only — never keyboard::with_mut,
    // and no need for the virtual-controller lock.
    match cmd {
        "geometry_ready" | "list_keys" | "get_key" | "stick_for_key" => {
            return handle_geometry_command(cmd, id, &req);
        }
        _ => {}
    }

    let session = session();
    let mut ctl = match session.lock() {
        Ok(g) => g,
        Err(_) => return err(id, "virtual controller lock poisoned"),
    };

    match cmd {
        "ping" => ok(id, json!({})),
        "neutral" => {
            ctl.neutral();
            ok(id, json!({}))
        }
        "get_state" => {
            let state = ctl.get_state_json();
            ok(id, json!({"state": state}))
        }
        "set_stick" => {
            let side = match req_side(&req) {
                Ok(s) => s,
                Err(e) => return err(id, e),
            };
            let x = match req_f32(&req, "x") {
                Ok(v) => v,
                Err(e) => return err(id, e),
            };
            let y = match req_f32(&req, "y") {
                Ok(v) => v,
                Err(e) => return err(id, e),
            };
            let duration_ms = req_duration(&req);
            ctl.set_stick(side, x, y, duration_ms);
            ok(id, json!({}))
        }
        "release_stick" => {
            let side = match req_side(&req) {
                Ok(s) => s,
                Err(e) => return err(id, e),
            };
            ctl.release_stick(side);
            ok(id, json!({}))
        }
        "set_pad" => {
            let side = match req_side(&req) {
                Ok(s) => s,
                Err(e) => return err(id, e),
            };
            let x = match req_f32(&req, "x") {
                Ok(v) => v,
                Err(e) => return err(id, e),
            };
            let y = match req_f32(&req, "y") {
                Ok(v) => v,
                Err(e) => return err(id, e),
            };
            let touching = match req.get("touching").and_then(|v| v.as_bool()) {
                Some(b) => b,
                None => return err(id, "missing bool field `touching`"),
            };
            let duration_ms = req_duration(&req);
            ctl.set_pad(side, x, y, touching, duration_ms);
            ok(id, json!({}))
        }
        "release_pad" => {
            let side = match req_side(&req) {
                Ok(s) => s,
                Err(e) => return err(id, e),
            };
            ctl.release_pad(side);
            ok(id, json!({}))
        }
        "press" => {
            let button = match req_button(&req) {
                Ok(b) => b,
                Err(e) => return err(id, e),
            };
            let duration_ms = req_duration(&req);
            ctl.set_button(button, true, duration_ms);
            ok(id, json!({}))
        }
        "release" => {
            let button = match req_button(&req) {
                Ok(b) => b,
                Err(e) => return err(id, e),
            };
            ctl.set_button(button, false, None);
            ok(id, json!({}))
        }
        "tap" => {
            let button = match req_button(&req) {
                Ok(b) => b,
                Err(e) => return err(id, e),
            };
            let duration_ms = Some(req_duration(&req).unwrap_or(50));
            ctl.set_button(button, true, duration_ms);
            ok(id, json!({}))
        }
        "set_trigger" => {
            let side = match req_side(&req) {
                Ok(s) => s,
                Err(e) => return err(id, e),
            };
            let value = match req.get("value") {
                None | Some(Value::Null) => None,
                Some(Value::Number(n)) => match n.as_u64() {
                    Some(v) if v <= 255 => Some(v as u8),
                    _ => return err(id, "`value` must be 0..=255 or null"),
                },
                Some(_) => return err(id, "`value` must be 0..=255 or null"),
            };
            let duration_ms = req_duration(&req);
            ctl.set_trigger(side, value, duration_ms);
            ok(id, json!({}))
        }
        other => err(id, format!("unknown cmd '{other}'")),
    }
}

fn geometry_not_ready(id: Value) -> Value {
    err_msg(
        id,
        "geometry_not_ready",
        "keyboard layout centres not captured; wait until after first keyboard draw",
    )
}

fn with_snap<F>(id: Value, req: &Value, f: F) -> Value
where
    F: FnOnce(&GeometrySnapshot) -> Value,
{
    let cell = geometry_snap::session();
    let guard = match cell.lock() {
        Ok(g) => g,
        Err(_) => return err(id, "geometry snapshot lock poisoned"),
    };
    let Some(snap) = guard.as_ref() else {
        return geometry_not_ready(id);
    };
    if let Some(want) = req.get("layout").and_then(|v| v.as_str()) {
        if want != snap.layout {
            return err(id, "layout_mismatch");
        }
    }
    f(snap)
}

fn resolve_key<'a>(
    snap: &'a GeometrySnapshot,
    req: &Value,
) -> Result<&'a geometry_snap::SnapKey, Value> {
    let key = req.get("key").and_then(|v| v.as_str());
    let row = req.get("row").and_then(|v| v.as_u64()).map(|v| v as usize);
    let col = req.get("col").and_then(|v| v.as_u64()).map(|v| v as usize);
    match (key, row, col) {
        (Some(k), r, c) => snap
            .lookup_key(k, r, c)
            .ok_or_else(|| err(id_of(req), "unknown_key")),
        (None, Some(r), Some(c)) => snap
            .lookup_key("", Some(r), Some(c))
            .ok_or_else(|| err(id_of(req), "unknown_key")),
        _ => Err(err(id_of(req), "provide `key` and/or both `row` and `col`")),
    }
}

fn hitbox_json(hb: &SnapHitBox) -> Value {
    match hb {
        SnapHitBox::Circle { x, y, r } => json!({"kind": "circle", "x": x, "y": y, "r": r}),
        SnapHitBox::Ellipse { x, y, rx, ry } => {
            json!({"kind": "ellipse", "x": x, "y": y, "rx": rx, "ry": ry})
        }
    }
}

fn centre_json(c: Option<(f32, f32)>) -> Value {
    match c {
        Some((x, y)) => json!({"x": x, "y": y}),
        None => Value::Null,
    }
}

fn range_json(r: Option<StickAabb>) -> Value {
    match r {
        Some(a) => json!({
            "min_x": a.min_x,
            "max_x": a.max_x,
            "min_y": a.min_y,
            "max_y": a.max_y
        }),
        None => Value::Null,
    }
}

fn handle_geometry_command(cmd: &str, id: Value, req: &Value) -> Value {
    match cmd {
        "geometry_ready" => {
            let cell = geometry_snap::session();
            let guard = match cell.lock() {
                Ok(g) => g,
                Err(_) => return err(id, "geometry snapshot lock poisoned"),
            };
            match guard.as_ref() {
                None => ok(id, json!({"ready": false})),
                Some(snap) => ok(
                    id,
                    json!({
                        "ready": true,
                        "layout": snap.layout,
                        "revision": snap.revision
                    }),
                ),
            }
        }
        "list_keys" => with_snap(id.clone(), req, |snap| {
            let keys: Vec<Value> = snap
                .keys
                .iter()
                .map(|k| {
                    json!({
                        "id": k.id,
                        "row": k.row,
                        "col": k.col,
                        "selectable": k.selectable,
                        "has_hitbox": k.hitbox.is_some()
                    })
                })
                .collect();
            ok(
                id,
                json!({
                    "layout": snap.layout,
                    "revision": snap.revision,
                    "keys": keys
                }),
            )
        }),
        "get_key" => with_snap(id.clone(), req, |snap| {
            let key = match resolve_key(snap, req) {
                Ok(k) => k,
                Err(e) => return e,
            };
            ok(
                id,
                json!({
                    "layout": snap.layout,
                    "revision": snap.revision,
                    "key": {
                        "id": key.id,
                        "row": key.row,
                        "col": key.col,
                        "selectable": key.selectable,
                        "centre": centre_json(key.centre),
                        "hitbox": key.hitbox.as_ref().map(hitbox_json).unwrap_or(Value::Null),
                        "left_rest": {"x": snap.left_rest.0, "y": snap.left_rest.1},
                        "right_rest": {"x": snap.right_rest.0, "y": snap.right_rest.1},
                        "scale_x": snap.scale_x,
                        "scale_y": snap.scale_y,
                        "stick_scale_x": snap.stick_scale_x,
                        "stick_scale_y": snap.stick_scale_y
                    }
                }),
            )
        }),
        "stick_for_key" => with_snap(id.clone(), req, |snap| {
            let side = match req_side(req) {
                Ok(s) => s,
                Err(e) => return err(id, e),
            };
            let include_range = req
                .get("include_range")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let key = match resolve_key(snap, req) {
                Ok(k) => k,
                Err(e) => return e,
            };

            let unreachable = |reason: &str| {
                ok(
                    id.clone(),
                    json!({
                        "side": side,
                        "key": key.id,
                        "row": key.row,
                        "col": key.col,
                        "x": Value::Null,
                        "y": Value::Null,
                        "clamped": false,
                        "reachable": false,
                        "reason": reason,
                        "range": Value::Null
                    }),
                )
            };

            if key.centre.is_none() {
                return unreachable("no_centre");
            }
            if key.hitbox.is_none() {
                return unreachable("no_hitbox");
            }
            if !key.selectable {
                return unreachable("not_selectable");
            }

            let (cx, cy) = key.centre.unwrap();
            let (raw_x, raw_y) = snap.stick_for_centre_raw(side, cx, cy);
            let clamped = raw_x.abs() > 1.0 || raw_y.abs() > 1.0;
            let mut x = raw_x.clamp(-1.0, 1.0);
            let mut y = raw_y.clamp(-1.0, 1.0);
            let mut reason: Option<&str> = None;
            let mut reachable = true;

            let hits = |sx: f32, sy: f32| -> bool {
                snap.nearest_key(side, (sx, sy))
                    .is_some_and(|n| n.row == key.row && n.col == key.col)
            };

            if !hits(x, y) {
                reason = Some(if clamped {
                    "miss_after_clamp"
                } else {
                    "out_of_reach"
                });
                reachable = false;
            }

            let range = if include_range {
                key.hitbox
                    .as_ref()
                    .and_then(|hb| snap.stick_aabb_for_hitbox(side, hb))
            } else {
                None
            };

            // Prefer a point that hits: if centre inverse misses but AABB exists, try AABB centre.
            if !reachable {
                if let Some(aabb) = range {
                    let ax = ((aabb.min_x + aabb.max_x) * 0.5).clamp(-1.0, 1.0);
                    let ay = ((aabb.min_y + aabb.max_y) * 0.5).clamp(-1.0, 1.0);
                    if hits(ax, ay) {
                        x = ax;
                        y = ay;
                        reachable = true;
                        reason = None;
                    }
                }
            }

            ok(
                id,
                json!({
                    "side": side,
                    "key": key.id,
                    "row": key.row,
                    "col": key.col,
                    "x": x,
                    "y": y,
                    "clamped": clamped,
                    "reachable": reachable,
                    "reason": reason,
                    "range": if include_range { range_json(range) } else { Value::Null }
                }),
            )
        }),
        other => err(id, format!("unknown cmd '{other}'")),
    }
}

fn req_side(req: &Value) -> Result<StickSide, String> {
    let s = req
        .get("side")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing string field `side`".to_owned())?;
    StickSide::parse(s)
}

fn req_f32(req: &Value, key: &str) -> Result<f32, String> {
    req.get(key)
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .ok_or_else(|| format!("missing number field `{key}`"))
}

fn req_duration(req: &Value) -> Option<u64> {
    req.get("duration_ms").and_then(|v| v.as_u64())
}

fn req_button(req: &Value) -> Result<ControllerButton, String> {
    let s = req
        .get("button")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing string field `button`".to_owned())?;
    ControllerButton::from_str(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::keyboard::geometry_snap::{GeometrySnapshot, SnapHitBox, SnapKey, SnapRect};

    fn inject_rest_snap() {
        let snap = GeometrySnapshot {
            layout: "main".into(),
            revision: 1,
            scale_x: 30.0,
            scale_y: 32.0,
            stick_scale_x: 3.0,
            stick_scale_y: 2.5,
            left_rest: (100.0, 100.0),
            right_rest: (300.0, 100.0),
            left_bounds: vec![SnapRect {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 200.0,
                max_y: 200.0,
            }],
            right_bounds: vec![SnapRect {
                min_x: 200.0,
                min_y: 0.0,
                max_x: 400.0,
                max_y: 200.0,
            }],
            keys: vec![
                SnapKey {
                    row: 0,
                    col: 0,
                    id: "d".into(),
                    selectable: true,
                    centre: Some((100.0, 100.0)),
                    hitbox: Some(SnapHitBox::Circle {
                        x: 100.0,
                        y: 100.0,
                        r: 33.75,
                    }),
                },
                SnapKey {
                    row: 0,
                    col: 1,
                    id: "e".into(),
                    selectable: true,
                    centre: Some((190.0, 100.0)),
                    hitbox: Some(SnapHitBox::Circle {
                        x: 190.0,
                        y: 100.0,
                        r: 33.75,
                    }),
                },
            ],
        };
        *geometry_snap::session().lock().unwrap() = Some(snap);
    }

    #[test]
    fn handle_neutral_set_stick_get_state_and_bad_cmd() {
        // Isolate shared session: start from neutral.
        let _ = handle_command(json!({"id": 1, "cmd": "neutral"}));

        let bad = handle_command(json!({"id": 2, "cmd": "nope"}));
        assert_eq!(bad["ok"], false);
        assert!(bad["error"].as_str().unwrap().contains("unknown cmd"));

        let set = handle_command(json!({
            "id": 3,
            "cmd": "set_stick",
            "side": "left",
            "x": 0.8,
            "y": 0.0
        }));
        assert_eq!(set["ok"], true);

        let state = handle_command(json!({"id": 4, "cmd": "get_state"}));
        assert_eq!(state["ok"], true);
        assert!((state["state"]["lx"].as_f64().unwrap() - 0.8).abs() < 1e-6);

        let _ = handle_command(json!({"id": 5, "cmd": "neutral"}));
    }

    #[test]
    fn geometry_ready_and_stick_for_key_on_rest() {
        geometry_snap::clear();
        let not_ready = handle_command(json!({"id": 10, "cmd": "geometry_ready"}));
        assert_eq!(not_ready["ok"], true);
        assert_eq!(not_ready["ready"], false);

        let missing = handle_command(json!({
            "id": 11,
            "cmd": "stick_for_key",
            "side": "left",
            "key": "d"
        }));
        assert_eq!(missing["ok"], false);
        assert_eq!(missing["error"], "geometry_not_ready");

        inject_rest_snap();

        let ready = handle_command(json!({"id": 12, "cmd": "geometry_ready"}));
        assert_eq!(ready["ok"], true);
        assert_eq!(ready["ready"], true);
        assert_eq!(ready["layout"], "main");

        let stick = handle_command(json!({
            "id": 13,
            "cmd": "stick_for_key",
            "side": "left",
            "key": "d"
        }));
        assert_eq!(stick["ok"], true);
        assert_eq!(stick["reachable"], true);
        assert!((stick["x"].as_f64().unwrap()).abs() < 1e-5);
        assert!((stick["y"].as_f64().unwrap()).abs() < 1e-5);

        geometry_snap::clear();
    }
}
