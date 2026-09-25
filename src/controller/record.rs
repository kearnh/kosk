//! Timestamped controller-input tape: encode/decode, numbered paths, off-thread writer.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::config;
use strum::VariantArray;

use crate::controller::{ControllerButton, ControllerInput, ControllerKind};

pub const TAPE_MAGIC: &str = "KOSKREC 1";
pub const CURRENT_TAPE_VERSION: u32 = 2;

/// Bit *i* in snapshots is `VARIANTS[i]`; append new buttons at the end of the enum.
pub(crate) const BUTTON_ORDER: &[ControllerButton] = ControllerButton::VARIANTS;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MappingScales {
    pub scale_x: f32,
    pub scale_y: f32,
    pub stick_scale_x: f32,
    pub stick_scale_y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TapeHeader {
    /// Missing `version` line in the file is 0.
    pub version: u32,
    pub current_layout: String,
    pub scales: MappingScales,
    /// Recorded config TOML (version >= 1). Blacklisted keys already stripped.
    pub config_toml: Option<String>,
    /// `(name, toml source)` sorted by name when encoded.
    pub layouts: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InputSnapshot {
    pub lx: f32,
    pub ly: f32,
    pub rx: f32,
    pub ry: f32,
    pub buttons: u32,
    pub lt: Option<u8>,
    pub rt: Option<u8>,
    pub lpad: Option<(f32, f32)>,
    pub rpad: Option<(f32, f32)>,
}

impl InputSnapshot {
    pub fn from_input(input: &dyn ControllerInput) -> Self {
        let (lx, ly) = input.left_stick();
        let (rx, ry) = input.right_stick();
        let mut buttons = 0u32;
        for (i, btn) in BUTTON_ORDER.iter().enumerate() {
            if input.query(*btn) {
                buttons |= 1 << i;
            }
        }
        Self {
            lx,
            ly,
            rx,
            ry,
            buttons,
            lt: input.trigger_left(),
            rt: input.trigger_right(),
            lpad: input.left_pad(),
            rpad: input.right_pad(),
        }
    }

    pub fn button(&self, btn: ControllerButton) -> bool {
        BUTTON_ORDER
            .iter()
            .position(|b| *b == btn)
            .is_some_and(|i| self.buttons & (1 << i) != 0)
    }

    pub fn any_nonzero(&self) -> bool {
        self.lx != 0.0
            || self.ly != 0.0
            || self.rx != 0.0
            || self.ry != 0.0
            || self.lpad.is_some()
            || self.rpad.is_some()
            || self.buttons != 0
            || self.lt.is_some()
            || self.rt.is_some()
    }
}

/// How [`PostMapInput::is_engaged`] treats an empty snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngagedPolicy {
    /// Always deliver (replay tape frames).
    Always,
    /// Deliver only when any axis/button/pad is active (MCP virtual).
    AnyNonZero,
}

/// Stored post-map coordinates — no re-warp / pad-origin (replay + MCP).
#[derive(Debug, Clone)]
pub struct PostMapInput {
    pub snap: InputSnapshot,
    engaged: EngagedPolicy,
}

impl PostMapInput {
    pub fn always(snap: InputSnapshot) -> Self {
        Self {
            snap,
            engaged: EngagedPolicy::Always,
        }
    }

    pub fn any_nonzero(snap: InputSnapshot) -> Self {
        Self {
            snap,
            engaged: EngagedPolicy::AnyNonZero,
        }
    }
}

impl ControllerInput for PostMapInput {
    fn left_stick_raw(&self) -> (f32, f32) {
        (self.snap.lx, self.snap.ly)
    }
    fn right_stick_raw(&self) -> (f32, f32) {
        (self.snap.rx, self.snap.ry)
    }
    fn left_stick(&self) -> (f32, f32) {
        (self.snap.lx, self.snap.ly)
    }
    fn right_stick(&self) -> (f32, f32) {
        (self.snap.rx, self.snap.ry)
    }
    fn left_pad_raw(&self) -> Option<(f32, f32)> {
        self.snap.lpad
    }
    fn right_pad_raw(&self) -> Option<(f32, f32)> {
        self.snap.rpad
    }
    fn left_pad(&self) -> Option<(f32, f32)> {
        self.snap.lpad
    }
    fn right_pad(&self) -> Option<(f32, f32)> {
        self.snap.rpad
    }
    fn trigger_left(&self) -> Option<u8> {
        self.snap.lt
    }
    fn trigger_right(&self) -> Option<u8> {
        self.snap.rt
    }
    fn query(&self, button: ControllerButton) -> bool {
        self.snap.button(button)
    }
    fn is_engaged(&self) -> bool {
        match self.engaged {
            EngagedPolicy::Always => true,
            EngagedPolicy::AnyNonZero => self.snap.any_nonzero(),
        }
    }
    fn family(&self) -> ControllerKind {
        ControllerKind::Replay
    }
    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
        Box::new(self.clone())
    }
}

/// One chip as shown. `current_word` is the typed token, not a dictionary hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedChip {
    pub text: String,
    pub current_word: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecordEvent {
    Idle {
        t_us: u64,
    },
    Snapshot {
        t_us: u64,
        snap: InputSnapshot,
    },
    Layout {
        t_us: u64,
        name: String,
    },
    Debounce {
        t_us: u64,
        accept: bool,
        source: String,
        elapsed_us: u64,
        armed: bool,
    },
    /// Chips applied for `prefix` (text before the cursor). Empty `chips` is an empty strip.
    Suggestions {
        t_us: u64,
        prefix: String,
        chips: Vec<RecordedChip>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tape {
    pub header: TapeHeader,
    pub events: Vec<RecordEvent>,
}

pub fn snapshot_from_option(input: &Option<Box<dyn ControllerInput>>, t_us: u64) -> RecordEvent {
    match input {
        None => RecordEvent::Idle { t_us },
        Some(inp) => RecordEvent::Snapshot {
            t_us,
            snap: InputSnapshot::from_input(inp.as_ref()),
        },
    }
}

pub fn validate_record_template(template: &str) -> Result<()> {
    let n = template.matches('%').count();
    if n != 1 {
        bail!("record_file must contain exactly one '%' (got {n})");
    }
    Ok(())
}

fn split_file_template(path: &Path) -> Result<(PathBuf, String, String)> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("record_file has no file name"))?;
    validate_record_template(name)?;
    let dir = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let (prefix, suffix) = name.split_once('%').expect("validated to contain one '%'");
    Ok((dir, prefix.to_string(), suffix.to_string()))
}

fn parse_numbered_name(name: &str, prefix: &str, suffix: &str) -> Option<u32> {
    let rest = name.strip_prefix(prefix)?;
    let digits = rest.strip_suffix(suffix)?;
    if digits.len() != 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Next numbered path for `template` (absolute or already-resolved). Empty dir → `000`.
pub fn next_record_path(template: &Path) -> Result<PathBuf> {
    let (dir, prefix, suffix) = split_file_template(template)?;
    if !dir.as_os_str().is_empty() {
        fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let mut max: Option<u32> = None;
    if dir.exists() {
        for ent in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
            let ent = ent?;
            let name = ent.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if let Some(n) = parse_numbered_name(name, &prefix, &suffix) {
                max = Some(max.map_or(n, |m| m.max(n)));
            }
        }
    }
    let next = match max {
        None => 0,
        Some(999) => bail!("record_file numbering exhausted (999)"),
        Some(n) => n + 1,
    };
    Ok(dir.join(format!("{prefix}{next:03}{suffix}")))
}

pub fn resolve_against_config_dir(rel: &str) -> Result<PathBuf> {
    let p = PathBuf::from(rel);
    if p.is_absolute() {
        return Ok(p);
    }
    let base = config::config_dir().ok_or_else(|| anyhow::anyhow!("config path not set"))?;
    Ok(base.join(p))
}

fn fmt_trigger(t: Option<u8>) -> String {
    match t {
        None => "-".to_string(),
        Some(v) => v.to_string(),
    }
}

fn parse_trigger(s: &str) -> Result<Option<u8>> {
    if s == "-" {
        return Ok(None);
    }
    Ok(Some(s.parse().with_context(|| format!("trigger '{s}'"))?))
}

fn fmt_pad(p: Option<(f32, f32)>) -> String {
    match p {
        None => "-".to_string(),
        Some((x, y)) => format!("{x},{y}"),
    }
}

fn parse_pad(s: &str) -> Result<Option<(f32, f32)>> {
    if s == "-" {
        return Ok(None);
    }
    let (xs, ys) = s
        .split_once(',')
        .ok_or_else(|| anyhow::anyhow!("pad '{s}'"))?;
    let x: f32 = xs.parse().with_context(|| format!("pad x '{s}'"))?;
    let y: f32 = ys.parse().with_context(|| format!("pad y '{s}'"))?;
    Ok(Some((x, y)))
}

fn write_len_prefixed_blob(
    out: &mut Vec<u8>,
    kind: &str,
    extra: Option<&str>,
    blob: &str,
) -> Result<()> {
    match extra {
        Some(extra) => writeln!(out, "{kind} {extra} {}", blob.len())?,
        None => writeln!(out, "{kind} {}", blob.len())?,
    }
    out.extend_from_slice(blob.as_bytes());
    out.push(b'\n');
    Ok(())
}

pub fn encode_header(header: &TapeHeader) -> Result<Vec<u8>> {
    if header.version > CURRENT_TAPE_VERSION {
        bail!(
            "cannot write recording version {} (max {CURRENT_TAPE_VERSION})",
            header.version
        );
    }
    if header.version == 0 && header.config_toml.is_some() {
        bail!("version 0 recordings must not include a config blob");
    }
    if header.version >= 1 && header.config_toml.is_none() {
        bail!("version {} recording is missing config", header.version);
    }

    let mut out = Vec::new();
    writeln!(out, "{TAPE_MAGIC}")?;
    if header.version != 0 {
        writeln!(out, "version {}", header.version)?;
    }
    writeln!(out, "current_layout {}", header.current_layout)?;
    writeln!(
        out,
        "scale {} {} {} {}",
        header.scales.scale_x,
        header.scales.scale_y,
        header.scales.stick_scale_x,
        header.scales.stick_scale_y
    )?;
    if let Some(toml) = &header.config_toml {
        write_len_prefixed_blob(&mut out, "config", None, toml)?;
    }
    let mut layouts = header.layouts.clone();
    layouts.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, toml) in &layouts {
        if name.split_whitespace().count() != 1 {
            bail!("layout name must be a single token, got {name:?}");
        }
        write_len_prefixed_blob(&mut out, "layout", Some(name), toml)?;
    }
    Ok(out)
}

pub fn encode_event(ev: &RecordEvent) -> String {
    match ev {
        RecordEvent::Idle { t_us } => format!("{t_us} idle"),
        RecordEvent::Snapshot { t_us, snap } => format!(
            "{t_us} {} {} {} {} {:x} {} {} {} {}",
            snap.lx,
            snap.ly,
            snap.rx,
            snap.ry,
            snap.buttons,
            fmt_trigger(snap.lt),
            fmt_trigger(snap.rt),
            fmt_pad(snap.lpad),
            fmt_pad(snap.rpad)
        ),
        RecordEvent::Layout { t_us, name } => format!("{t_us} layout {name}"),
        RecordEvent::Debounce {
            t_us,
            accept,
            source,
            elapsed_us,
            armed,
        } => {
            let verdict = if *accept { "accept" } else { "drop" };
            format!("{t_us} debounce {verdict} {source} elapsed_us={elapsed_us} armed={armed}")
        }
        RecordEvent::Suggestions {
            t_us,
            prefix,
            chips,
        } => {
            let mut line = format!("{t_us} suggestions {}", encode_len_text(prefix));
            for chip in chips {
                line.push(' ');
                if chip.current_word {
                    line.push('+');
                }
                line.push_str(&encode_len_text(&chip.text));
            }
            line
        }
    }
}

fn encode_len_text(text: &str) -> String {
    format!("{}:{text}", text.len())
}

fn parse_event_line(line: &str) -> Result<RecordEvent> {
    let line = line.trim();
    let (t_s, rest) = line
        .split_once(' ')
        .ok_or_else(|| anyhow::anyhow!("empty event line"))?;
    let t_us: u64 = t_s.parse().with_context(|| format!("timestamp '{t_s}'"))?;
    if rest == "idle" {
        return Ok(RecordEvent::Idle { t_us });
    }
    if let Some(name) = rest.strip_prefix("layout ") {
        return Ok(RecordEvent::Layout {
            t_us,
            name: name.trim().to_string(),
        });
    }
    if let Some(rest) = rest.strip_prefix("debounce ") {
        let (verdict, rest) = rest
            .split_once(' ')
            .ok_or_else(|| anyhow::anyhow!("malformed debounce line"))?;
        let accept = match verdict {
            "accept" => true,
            "drop" => false,
            _ => bail!("debounce verdict must be accept or drop, got {verdict}"),
        };
        let armed_at = rest
            .rfind(" armed=")
            .ok_or_else(|| anyhow::anyhow!("debounce line missing armed="))?;
        let elapsed_at = rest[..armed_at]
            .rfind(" elapsed_us=")
            .ok_or_else(|| anyhow::anyhow!("debounce line missing elapsed_us="))?;
        let source = rest[..elapsed_at].trim().to_string();
        let elapsed_us: u64 = rest[elapsed_at + " elapsed_us=".len()..armed_at]
            .parse()
            .context("elapsed_us")?;
        let armed: bool = rest[armed_at + " armed=".len()..]
            .parse()
            .context("armed")?;
        return Ok(RecordEvent::Debounce {
            t_us,
            accept,
            source,
            elapsed_us,
            armed,
        });
    }
    if let Some(rest) = rest.strip_prefix("suggestions ") {
        return parse_suggestions(t_us, rest);
    }
    let mut parts = rest.split_whitespace();
    let lx: f32 = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing lx"))?
        .parse()?;
    let ly: f32 = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing ly"))?
        .parse()?;
    let rx: f32 = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing rx"))?
        .parse()?;
    let ry: f32 = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing ry"))?
        .parse()?;
    let buttons = u32::from_str_radix(
        parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing buttons"))?,
        16,
    )?;
    let lt = parse_trigger(parts.next().ok_or_else(|| anyhow::anyhow!("missing lt"))?)?;
    let rt = parse_trigger(parts.next().ok_or_else(|| anyhow::anyhow!("missing rt"))?)?;
    let (lpad, rpad) = match (parts.next(), parts.next()) {
        (Some(lp), Some(rp)) => (parse_pad(lp)?, parse_pad(rp)?),
        _ => (None, None),
    };
    Ok(RecordEvent::Snapshot {
        t_us,
        snap: InputSnapshot {
            lx,
            ly,
            rx,
            ry,
            buttons,
            lt,
            rt,
            lpad,
            rpad,
        },
    })
}

fn parse_suggestions(t_us: u64, rest: &str) -> Result<RecordEvent> {
    let (prefix, _, mut rest) = take_len_field(rest, false)?;
    let mut chips = Vec::new();
    while !rest.trim_start().is_empty() {
        let (text, current_word, next) = take_len_field(rest, true)?;
        chips.push(RecordedChip { text, current_word });
        rest = next;
    }
    Ok(RecordEvent::Suggestions {
        t_us,
        prefix,
        chips,
    })
}

/// `N:text`, or `+N:text` when `allow_mark`. Length is bytes.
fn take_len_field(input: &str, allow_mark: bool) -> Result<(String, bool, &str)> {
    let rest = input.trim_start();
    if rest.is_empty() {
        bail!("missing length-prefixed field");
    }

    let (current_word, rest) = if allow_mark && rest.starts_with('+') {
        (true, &rest[1..])
    } else {
        (false, rest)
    };
    let colon = rest
        .find(':')
        .ok_or_else(|| anyhow::anyhow!("missing ':' in length-prefixed field"))?;
    let n: usize = rest[..colon].parse().context("length prefix")?;
    let after = &rest[colon + 1..];
    if after.len() < n || !after.is_char_boundary(n) {
        bail!("length-prefixed field truncated");
    }

    Ok((after[..n].to_string(), current_word, &after[n..]))
}

fn read_len_prefixed_blob(reader: &mut impl Read, nbytes: usize) -> Result<String> {
    let mut buf = vec![0u8; nbytes];
    reader
        .read_exact(&mut buf)
        .context("length-prefixed blob")?;
    let toml = String::from_utf8(buf).context("blob utf-8")?;
    let mut sep = [0u8; 1];
    reader.read_exact(&mut sep).context("blob separator")?;
    if sep[0] != b'\n' && sep[0] != b'\r' {
        bail!("expected newline after blob");
    }
    if sep[0] == b'\r' {
        let mut n2 = [0u8; 1];
        reader.read_exact(&mut n2)?;
    }
    Ok(toml)
}

pub fn parse_tape(reader: impl Read) -> Result<Tape> {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if line.trim() != TAPE_MAGIC {
        bail!(
            "not a kosk recording (expected {TAPE_MAGIC:?}, got {:?})",
            line.trim()
        );
    }

    line.clear();
    reader.read_line(&mut line)?;
    let trimmed = line.trim();
    let (version, current_layout) = if let Some(vs) = trimmed.strip_prefix("version ") {
        let version: u32 = vs
            .parse()
            .with_context(|| format!("recording version '{vs}'"))?;
        if version > CURRENT_TAPE_VERSION {
            bail!("unsupported recording version {version} (max {CURRENT_TAPE_VERSION})");
        }
        line.clear();
        reader.read_line(&mut line)?;
        let current_layout = line
            .trim()
            .strip_prefix("current_layout ")
            .ok_or_else(|| anyhow::anyhow!("missing current_layout"))?
            .to_string();
        (version, current_layout)
    } else {
        let current_layout = trimmed
            .strip_prefix("current_layout ")
            .ok_or_else(|| anyhow::anyhow!("missing current_layout"))?
            .to_string();
        (0, current_layout)
    };

    line.clear();
    reader.read_line(&mut line)?;
    let scale = line
        .trim()
        .strip_prefix("scale ")
        .ok_or_else(|| anyhow::anyhow!("missing scale"))?;
    let mut sp = scale.split_whitespace();
    let scales = MappingScales {
        scale_x: sp
            .next()
            .ok_or_else(|| anyhow::anyhow!("scale_x"))?
            .parse()?,
        scale_y: sp
            .next()
            .ok_or_else(|| anyhow::anyhow!("scale_y"))?
            .parse()?,
        stick_scale_x: sp
            .next()
            .ok_or_else(|| anyhow::anyhow!("stick_scale_x"))?
            .parse()?,
        stick_scale_y: sp
            .next()
            .ok_or_else(|| anyhow::anyhow!("stick_scale_y"))?
            .parse()?,
    };

    let mut layouts = Vec::new();
    let mut config_toml = None;
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            continue;
        }
        if let Some(len_s) = trimmed.strip_prefix("config ") {
            if version == 0 {
                bail!("version 0 recordings must not include a config blob");
            }
            if config_toml.is_some() {
                bail!("duplicate config blob");
            }
            let nbytes: usize = len_s.parse().context("config byte length")?;
            config_toml = Some(read_len_prefixed_blob(&mut reader, nbytes)?);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("layout ") {
            let (name, len_s) = rest
                .rsplit_once(' ')
                .ok_or_else(|| anyhow::anyhow!("malformed layout header"))?;
            let nbytes: usize = len_s.parse().context("layout byte length")?;
            let toml = read_len_prefixed_blob(&mut reader, nbytes)?;
            layouts.push((name.to_string(), toml));
            continue;
        }
        if version >= 1 && config_toml.is_none() {
            bail!("version {version} recording is missing config");
        }
        let mut events = vec![parse_event_line(trimmed)?];
        for l in reader.lines() {
            let l = l?;
            if l.trim().is_empty() {
                continue;
            }
            events.push(parse_event_line(&l)?);
        }
        return Ok(Tape {
            header: TapeHeader {
                version,
                current_layout,
                scales,
                config_toml,
                layouts,
            },
            events,
        });
    }

    if version >= 1 && config_toml.is_none() {
        bail!("version {version} recording is missing config");
    }
    Ok(Tape {
        header: TapeHeader {
            version,
            current_layout,
            scales,
            config_toml,
            layouts,
        },
        events: Vec::new(),
    })
}

pub struct InputRecorder {
    tx: Option<SyncSender<RecordEvent>>,
    thread: Option<JoinHandle<()>>,
    t0: Instant,
    dropped: AtomicBool,
}

impl InputRecorder {
    pub fn start(path: PathBuf, header: TapeHeader) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel(65_536);
        let header_bytes = encode_header(&header)?;
        let thread = thread::spawn(move || {
            let file = match File::create(&path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("warn: could not create recording {}: {e}", path.display());
                    return;
                }
            };
            let mut w = BufWriter::with_capacity(64 * 1024, file);
            if let Err(e) = w.write_all(&header_bytes) {
                eprintln!("warn: recording header write failed: {e}");
                return;
            }
            loop {
                match rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(ev) => {
                        if writeln!(w, "{}", encode_event(&ev)).is_err() {
                            eprintln!("warn: recording write failed");
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        let _ = w.flush();
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        let _ = w.flush();
                        break;
                    }
                }
            }
        });
        Ok(Self {
            tx: Some(tx),
            thread: Some(thread),
            t0: Instant::now(),
            dropped: AtomicBool::new(false),
        })
    }

    pub fn t_us(&self) -> u64 {
        self.t0.elapsed().as_micros() as u64
    }

    pub fn try_send(&self, ev: RecordEvent) {
        let Some(tx) = &self.tx else {
            return;
        };
        if tx.try_send(ev).is_err() && !self.dropped.swap(true, Ordering::Relaxed) {
            eprintln!("warn: input recorder dropped events (channel full or writer gone)");
        }
    }
}

impl Drop for InputRecorder {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

pub struct RecordSession {
    is_replay: AtomicBool,
    inner: Mutex<Option<InputRecorder>>,
}

impl RecordSession {
    fn new() -> Self {
        Self {
            is_replay: AtomicBool::new(false),
            inner: Mutex::new(None),
        }
    }

    pub fn set_replay(&self, yes: bool) {
        self.is_replay.store(yes, Ordering::Relaxed);
    }

    pub fn is_replay(&self) -> bool {
        self.is_replay.load(Ordering::Relaxed)
    }

    pub fn is_recording(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }

    pub fn start(&self, path: PathBuf, header: TapeHeader) -> Result<()> {
        let rec = InputRecorder::start(path, header)?;
        *self.inner.lock().unwrap() = Some(rec);
        Ok(())
    }

    pub fn stop(&self) {
        *self.inner.lock().unwrap() = None;
    }

    pub fn tap_input(&self, input: &Option<Box<dyn ControllerInput>>) {
        let g = self.inner.lock().unwrap();
        let Some(rec) = g.as_ref() else {
            return;
        };
        let t_us = rec.t_us();
        rec.try_send(snapshot_from_option(input, t_us));
    }

    pub fn tap_layout(&self, name: &str) {
        let g = self.inner.lock().unwrap();
        let Some(rec) = g.as_ref() else {
            return;
        };
        rec.try_send(RecordEvent::Layout {
            t_us: rec.t_us(),
            name: name.to_string(),
        });
    }

    pub fn tap_debounce(&self, accept: bool, source: &str, elapsed_us: u64, armed: bool) {
        let g = self.inner.lock().unwrap();
        let Some(rec) = g.as_ref() else {
            return;
        };
        rec.try_send(RecordEvent::Debounce {
            t_us: rec.t_us(),
            accept,
            source: source.to_string(),
            elapsed_us,
            armed,
        });
    }

    pub fn tap_suggestions(&self, prefix: &str, chips: &[RecordedChip]) {
        let g = self.inner.lock().unwrap();
        let Some(rec) = g.as_ref() else {
            return;
        };
        rec.try_send(RecordEvent::Suggestions {
            t_us: rec.t_us(),
            prefix: prefix.to_string(),
            chips: chips.to_vec(),
        });
    }
}

static SESSION: OnceLock<Arc<RecordSession>> = OnceLock::new();

pub fn session() -> Arc<RecordSession> {
    SESSION
        .get_or_init(|| Arc::new(RecordSession::new()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn roundtrip(tape: &Tape) -> Tape {
        let mut bytes = encode_header(&tape.header).unwrap();
        for ev in &tape.events {
            writeln!(bytes, "{}", encode_event(ev)).unwrap();
        }
        parse_tape(bytes.as_slice()).unwrap()
    }

    fn sample_header() -> TapeHeader {
        TapeHeader {
            version: 0,
            current_layout: "main".into(),
            scales: MappingScales {
                scale_x: 30.0,
                scale_y: 32.0,
                stick_scale_x: 3.0,
                stick_scale_y: 2.5,
            },
            config_toml: None,
            layouts: vec![
                ("other".into(), "pad_x = 1\n[[rows]]\nitems = []\n".into()),
                ("main".into(), "pad_x = 0.1\n[[rows]]\nitems = []\n".into()),
            ],
        }
    }

    #[test]
    fn header_and_events_round_trip() {
        let tape = Tape {
            header: sample_header(),
            events: vec![
                RecordEvent::Idle { t_us: 10 },
                RecordEvent::Snapshot {
                    t_us: 20,
                    snap: InputSnapshot {
                        lx: 0.25,
                        ly: -0.5,
                        rx: 0.0,
                        ry: 1.0,
                        buttons: 0x1a2b,
                        lt: None,
                        rt: Some(180),
                        lpad: None,
                        rpad: None,
                    },
                },
                RecordEvent::Layout {
                    t_us: 30,
                    name: "other".into(),
                },
                RecordEvent::Debounce {
                    t_us: 40,
                    accept: false,
                    source: "options + faceTop".into(),
                    elapsed_us: 12000,
                    armed: true,
                },
            ],
        };
        let parsed = roundtrip(&tape);
        assert_eq!(parsed.header.current_layout, "main");
        assert_eq!(parsed.header.version, 0);
        assert!(parsed.header.config_toml.is_none());
        assert_eq!(parsed.header.scales, tape.header.scales);
        let names: HashSet<_> = parsed
            .header
            .layouts
            .iter()
            .map(|(n, _)| n.as_str())
            .collect();
        assert!(names.contains("main") && names.contains("other"));
        assert_eq!(parsed.events.len(), 4);
        assert!(matches!(parsed.events[0], RecordEvent::Idle { t_us: 10 }));
        match &parsed.events[1] {
            RecordEvent::Snapshot { snap, .. } => {
                assert!((snap.lx - 0.25).abs() < 1e-5);
                assert_eq!(snap.buttons, 0x1a2b);
                assert_eq!(snap.rt, Some(180));
            }
            other => panic!("expected snapshot, got {other:?}"),
        }
        match &parsed.events[3] {
            RecordEvent::Debounce {
                accept,
                source,
                elapsed_us,
                armed,
                ..
            } => {
                assert!(!*accept);
                assert_eq!(source, "options + faceTop");
                assert_eq!(*elapsed_us, 12000);
                assert!(*armed);
            }
            other => panic!("expected debounce, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_pads_optional_and_round_trip() {
        match parse_event_line("20 0.25 -0.5 0 1 1a2b - 180").unwrap() {
            RecordEvent::Snapshot { snap, .. } => {
                assert_eq!(snap.lpad, None);
                assert_eq!(snap.rpad, None);
            }
            other => panic!("expected snapshot, got {other:?}"),
        }
        match parse_event_line("20 0.25 -0.5 0 1 1a2b - 180 - 0.1,-0.2").unwrap() {
            RecordEvent::Snapshot { snap, .. } => {
                assert_eq!(snap.lpad, None);
                assert_eq!(snap.rpad, Some((0.1, -0.2)));
            }
            other => panic!("expected snapshot, got {other:?}"),
        }
        let ev = RecordEvent::Snapshot {
            t_us: 20,
            snap: InputSnapshot {
                lx: 0.0,
                ly: 0.0,
                rx: 0.0,
                ry: 0.0,
                buttons: 0,
                lt: None,
                rt: None,
                lpad: None,
                rpad: Some((0.1, -0.2)),
            },
        };
        assert_eq!(parse_event_line(&encode_event(&ev)).unwrap(), ev);
    }

    #[test]
    fn debounce_line_is_skipped_by_replay_filter() {
        let ev = RecordEvent::Debounce {
            t_us: 1,
            accept: true,
            source: "triggerLeft".into(),
            elapsed_us: 0,
            armed: false,
        };
        let line = encode_event(&ev);
        let parsed = parse_event_line(&line).unwrap();
        assert!(matches!(parsed, RecordEvent::Debounce { accept: true, .. }));
    }

    #[test]
    fn next_record_path_empty_dir_is_zero() {
        let dir = std::env::temp_dir().join(format!(
            "kosk-rec-empty-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let template = dir.join("kosk-%.krec");
        let p = next_record_path(&template).unwrap();
        assert_eq!(p.file_name().unwrap(), "kosk-000.krec");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn next_record_path_max_plus_one() {
        let dir = std::env::temp_dir().join(format!(
            "kosk-rec-max-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("kosk-000.krec")).unwrap();
        File::create(dir.join("kosk-002.krec")).unwrap();
        File::create(dir.join("unrelated.txt")).unwrap();
        let template = dir.join("kosk-%.krec");
        let p = next_record_path(&template).unwrap();
        assert_eq!(p.file_name().unwrap(), "kosk-003.krec");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn next_record_path_refuses_999() {
        let dir = std::env::temp_dir().join(format!(
            "kosk-rec-full-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("kosk-999.krec")).unwrap();
        let template = dir.join("kosk-%.krec");
        let err = next_record_path(&template).unwrap_err().to_string();
        assert!(err.contains("999"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_percent_count() {
        assert!(validate_record_template("kosk-%.krec").is_ok());
        assert!(validate_record_template("kosk.krec").is_err());
        assert!(validate_record_template("kosk-%-% .krec").is_err());
    }

    #[test]
    fn button_bitmask_covers_paddles() {
        let mut snap = InputSnapshot {
            lx: 0.0,
            ly: 0.0,
            rx: 0.0,
            ry: 0.0,
            buttons: 0,
            lt: None,
            rt: None,
            lpad: None,
            rpad: None,
        };
        for (i, btn) in BUTTON_ORDER.iter().enumerate() {
            snap.buttons = 1 << i;
            assert!(snap.button(*btn), "{btn:?}");
        }
        assert!(BUTTON_ORDER.contains(&ControllerButton::L4));
        assert!(BUTTON_ORDER.contains(&ControllerButton::R5));
        assert!(BUTTON_ORDER.contains(&ControllerButton::PadLeft));
        assert!(BUTTON_ORDER.contains(&ControllerButton::QuickAccess));
    }

    #[test]
    fn missing_version_line_is_v0() {
        let layout = "pad_x = 0\n[[rows]]\nitems = []\n";
        let mut bytes = format!(
            "{TAPE_MAGIC}\ncurrent_layout main\nscale 30 32 3 2.5\nlayout main {}\n",
            layout.len()
        )
        .into_bytes();
        bytes.extend_from_slice(layout.as_bytes());
        bytes.push(b'\n');
        let tape = parse_tape(bytes.as_slice()).unwrap();
        assert_eq!(tape.header.version, 0);
        assert!(tape.header.config_toml.is_none());
        assert_eq!(tape.header.current_layout, "main");
    }

    fn sample_v1_header() -> TapeHeader {
        TapeHeader {
            version: 1,
            current_layout: "main".into(),
            scales: MappingScales {
                scale_x: 30.0,
                scale_y: 32.0,
                stick_scale_x: 3.0,
                stick_scale_y: 2.5,
            },
            config_toml: Some("event_debounce_ms = 123\n".into()),
            layouts: vec![("main".into(), "pad_x = 0.1\n[[rows]]\nitems = []\n".into())],
        }
    }

    #[test]
    fn v1_header_round_trip_includes_config() {
        let parsed = roundtrip(&Tape {
            header: sample_v1_header(),
            events: vec![RecordEvent::Idle { t_us: 1 }],
        });
        assert_eq!(parsed.header.version, 1);
        assert_eq!(
            parsed.header.config_toml.as_deref(),
            Some("event_debounce_ms = 123\n")
        );
        assert_eq!(parsed.events.len(), 1);
    }

    #[test]
    fn suggestions_line_round_trip() {
        let ev = RecordEvent::Suggestions {
            t_us: 1000,
            prefix: "hello you".into(),
            chips: vec![
                RecordedChip {
                    text: "hello".into(),
                    current_word: false,
                },
                RecordedChip {
                    text: "you".into(),
                    current_word: true,
                },
            ],
        };
        assert_eq!(parse_event_line(&encode_event(&ev)).unwrap(), ev);

        let empty = RecordEvent::Suggestions {
            t_us: 2,
            prefix: "hel".into(),
            chips: vec![],
        };
        assert_eq!(parse_event_line(&encode_event(&empty)).unwrap(), empty);

        let parsed = roundtrip(&Tape {
            header: TapeHeader {
                version: CURRENT_TAPE_VERSION,
                current_layout: "main".into(),
                scales: MappingScales {
                    scale_x: 1.0,
                    scale_y: 1.0,
                    stick_scale_x: 1.0,
                    stick_scale_y: 1.0,
                },
                config_toml: Some("event_debounce_ms = 1\n".into()),
                layouts: vec![("main".into(), "pad_x = 0\n[[rows]]\nitems = []\n".into())],
            },
            events: vec![ev, empty],
        });
        assert_eq!(parsed.header.version, CURRENT_TAPE_VERSION);
        assert_eq!(parsed.events.len(), 2);
    }

    #[test]
    fn v1_missing_config_errors() {
        let layout = "pad_x = 0\n[[rows]]\nitems = []\n";
        let mut bytes = format!(
            "{TAPE_MAGIC}\nversion 1\ncurrent_layout main\nscale 1 1 1 1\nlayout main {}\n",
            layout.len()
        )
        .into_bytes();
        bytes.extend_from_slice(layout.as_bytes());
        bytes.push(b'\n');
        let err = parse_tape(bytes.as_slice()).unwrap_err().to_string();
        assert!(err.contains("missing config"), "{err}");
    }

    #[test]
    fn unknown_version_errors() {
        let bytes =
            format!("{TAPE_MAGIC}\nversion 99\ncurrent_layout main\nscale 1 1 1 1\n").into_bytes();
        let err = parse_tape(bytes.as_slice()).unwrap_err().to_string();
        assert!(err.contains("unsupported recording version 99"), "{err}");
    }

    #[test]
    fn v0_must_not_contain_config_blob() {
        let cfg = "event_debounce_ms = 1\n";
        let layout = "pad_x = 0\n[[rows]]\nitems = []\n";
        let mut bytes = format!(
            "{TAPE_MAGIC}\ncurrent_layout main\nscale 1 1 1 1\nconfig {}\n",
            cfg.len()
        )
        .into_bytes();
        bytes.extend_from_slice(cfg.as_bytes());
        bytes.push(b'\n');
        bytes.extend(format!("layout main {}\n", layout.len()).into_bytes());
        bytes.extend_from_slice(layout.as_bytes());
        bytes.push(b'\n');
        let err = parse_tape(bytes.as_slice()).unwrap_err().to_string();
        assert!(err.contains("version 0"), "{err}");
    }
}
