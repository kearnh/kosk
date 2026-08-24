//! Loopback NDJSON control server for kosk-mcp virtual controller injection.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::str::FromStr;
use std::thread;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::controller::virtual_ctl::{session, StickSide};
use crate::controller::ControllerButton;

/// Spawn the accept loop on a dedicated thread. Fails if bind fails.
pub fn spawn(addr: SocketAddr) -> Result<()> {
    let listener = TcpListener::bind(addr)
        .with_context(|| format!("bind kosk controller MCP port {addr}"))?;
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
}
