//! Outgoing key/text injection: Enigo, or a timestamped log.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use enigo::{Enigo, Keyboard as _};

use crate::config::{self, KeySinkConfig};
use crate::controller::record;
use crate::controller::replay;

pub trait KeySink: Send {
    fn key(&mut self, key: enigo::Key, direction: enigo::Direction) -> Result<()>;
    fn text(&mut self, text: &str) -> Result<()>;
}

pub struct EnigoSink(Enigo);

impl EnigoSink {
    pub fn new() -> Result<Self> {
        Ok(Self(
            Enigo::new(&Default::default()).context("create Enigo keyboard sink")?,
        ))
    }
}

impl KeySink for EnigoSink {
    fn key(&mut self, key: enigo::Key, direction: enigo::Direction) -> Result<()> {
        self.0
            .key(key, direction)
            .map_err(|e| anyhow::anyhow!("enigo key: {e}"))
    }

    fn text(&mut self, text: &str) -> Result<()> {
        self.0
            .text(text)
            .map_err(|e| anyhow::anyhow!("enigo text: {e}"))
    }
}

pub struct LogSink {
    writer: BufWriter<Box<dyn Write + Send>>,
    t0: Instant,
}

impl LogSink {
    pub fn with_writer(
        writer: impl Write + Send + 'static,
        write_git_header: bool,
    ) -> Result<Self> {
        let mut writer = BufWriter::new(Box::new(writer) as Box<dyn Write + Send>);
        if write_git_header {
            writer.write_all(git_header_line().as_bytes())?;
            writer.flush()?;
        }
        Ok(Self {
            writer,
            t0: Instant::now(),
        })
    }

    pub fn open_stdout() -> Result<Self> {
        Self::with_writer(std::io::stdout(), false)
    }

    pub fn open_file(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create keys log dir {}", parent.display()))?;
            }
        }
        let file =
            File::create(path).with_context(|| format!("create keys log {}", path.display()))?;
        Self::with_writer(file, true)
    }

    fn t_us(&self) -> u64 {
        if let Some(origin) = replay::playback_origin() {
            origin.elapsed().as_micros() as u64
        } else {
            self.t0.elapsed().as_micros() as u64
        }
    }

    fn write_line(&mut self, line: &str) -> Result<()> {
        writeln!(self.writer, "{line}")?;
        self.writer.flush()?;
        Ok(())
    }
}

impl KeySink for LogSink {
    fn key(&mut self, key: enigo::Key, direction: enigo::Direction) -> Result<()> {
        self.write_line(&format!("{} key {:?} {:?}", self.t_us(), key, direction))
    }

    fn text(&mut self, text: &str) -> Result<()> {
        self.write_line(&format!("{} text {text}", self.t_us()))
    }
}

fn git_header_line() -> String {
    let commit = env!("GIT_COMMIT");
    if env!("GIT_DIRTY") == "1" {
        format!("# git {commit} dirty\n")
    } else {
        format!("# git {commit}\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeysLogTarget {
    Stdout,
    File(PathBuf),
}

pub(crate) fn keys_log_target(
    spec: &Path,
    resolve_against_config_dir: bool,
) -> Result<KeysLogTarget> {
    if spec.as_os_str() == "-" {
        return Ok(KeysLogTarget::Stdout);
    }
    if resolve_against_config_dir {
        let s = spec
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("keys log path is not UTF-8"))?;
        Ok(KeysLogTarget::File(record::resolve_against_config_dir(s)?))
    } else {
        Ok(KeysLogTarget::File(spec.to_path_buf()))
    }
}

fn open_log_target(target: KeysLogTarget) -> Result<Box<dyn KeySink>> {
    match target {
        KeysLogTarget::Stdout => Ok(Box::new(LogSink::open_stdout()?)),
        KeysLogTarget::File(path) => Ok(Box::new(LogSink::open_file(&path)?)),
    }
}

/// Build the process-wide key sink from CLI `--keys-log` or `[key_sink]`.
pub fn open_key_sink() -> Result<Box<dyn KeySink>> {
    if let Some(path) = config::cli_keys_log() {
        return open_log_target(keys_log_target(&path, false)?);
    }
    match config::get().key_sink {
        KeySinkConfig::Enigo => Ok(Box::new(EnigoSink::new()?)),
        KeySinkConfig::Log { file } => open_log_target(keys_log_target(Path::new(&file), true)?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enigo::{Direction, Key};
    use std::io::{Cursor, Write};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn captured_sink(write_header: bool) -> (LogSink, Arc<Mutex<Vec<u8>>>) {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let sink = LogSink::with_writer(Capture(buf.clone()), write_header).unwrap();
        (sink, buf)
    }

    fn captured_text(buf: &Arc<Mutex<Vec<u8>>>) -> String {
        String::from_utf8(buf.lock().unwrap().clone()).unwrap()
    }

    fn data_lines(text: &str) -> Vec<&str> {
        text.lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect()
    }

    #[test]
    fn log_sink_round_trip_lines() {
        replay::clear_playback_origin();
        let path = std::env::temp_dir().join(format!(
            "kosk-keysink-roundtrip-{}-{}.log",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let mut sink = LogSink::open_file(&path).unwrap();
            sink.key(Key::Shift, Direction::Press).unwrap();
            sink.key(Key::Unicode('a'), Direction::Click).unwrap();
            sink.text("hello").unwrap();
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(
            text.starts_with("# git "),
            "file log should start with git header, got {text:?}"
        );
        let lines = data_lines(&text);
        assert_eq!(lines.len(), 3, "{text}");
        assert!(lines[0].contains("key Shift Press"), "{}", lines[0]);
        assert!(lines[1].contains("key Unicode('a') Click"), "{}", lines[1]);
        assert!(lines[2].contains("text hello"), "{}", lines[2]);
        for line in &lines {
            line.split_whitespace()
                .next()
                .unwrap()
                .parse::<u64>()
                .unwrap();
        }
    }

    #[test]
    fn file_log_has_git_header_stdout_style_does_not() {
        replay::clear_playback_origin();
        let (mut file_style, file_buf) = captured_sink(true);
        file_style.text("x").unwrap();
        let with_header = captured_text(&file_buf);
        assert!(with_header.starts_with("# git "), "{with_header:?}");
        assert!(with_header.contains("text x"), "{with_header:?}");

        let (mut stdout_style, stdout_buf) = captured_sink(false);
        stdout_style.text("x").unwrap();
        let without = captured_text(&stdout_buf);
        assert!(
            !without.starts_with('#'),
            "stdout-style writer must not get a header: {without:?}"
        );
        assert!(without.contains("text x"), "{without:?}");
    }

    #[test]
    fn dash_path_is_stdout() {
        assert_eq!(
            keys_log_target(Path::new("-"), false).unwrap(),
            KeysLogTarget::Stdout
        );
        assert_eq!(
            keys_log_target(Path::new("-"), true).unwrap(),
            KeysLogTarget::Stdout
        );
        match keys_log_target(Path::new("captures/keys.log"), false).unwrap() {
            KeysLogTarget::File(p) => assert_eq!(p, PathBuf::from("captures/keys.log")),
            KeysLogTarget::Stdout => panic!("expected file target"),
        }
    }

    #[test]
    fn timestamps_follow_playback_origin_else_local_t0() {
        replay::clear_playback_origin();
        let (mut sink, buf) = captured_sink(false);
        sink.key(Key::Shift, Direction::Press).unwrap();
        let local = captured_text(&buf);
        let t_local: u64 = local.split_whitespace().next().unwrap().parse().unwrap();
        assert!(
            t_local < 50_000,
            "local t0 should be near zero, got {t_local}"
        );

        buf.lock().unwrap().clear();
        let origin = Instant::now()
            .checked_sub(Duration::from_millis(250))
            .expect("clock went backwards");
        replay::set_playback_origin(origin);
        sink.key(Key::Shift, Direction::Release).unwrap();
        let replayed = captured_text(&buf);
        let t_replay: u64 = replayed.split_whitespace().next().unwrap().parse().unwrap();
        assert!(
            t_replay >= 200_000,
            "replay origin elapsed should be ~250ms, got {t_replay}"
        );
        replay::clear_playback_origin();
    }

    #[test]
    fn with_writer_cursor_compiles_as_in_memory_stdout() {
        replay::clear_playback_origin();
        let mut sink = LogSink::with_writer(Cursor::new(Vec::<u8>::new()), false).unwrap();
        sink.text("ok").unwrap();
    }
}
