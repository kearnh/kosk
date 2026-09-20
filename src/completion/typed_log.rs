use super::settings::CompletionKeyboardConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogEvent {
    Char(char),
    Text,
    Backspace,
    Enter,
    Arrow,
    Paste,
    CtrlAlt,
}

#[derive(Debug, Clone)]
pub struct TypedLog {
    text: String,
    armed: bool,
    last_injected: Option<String>,
    restore_token: Option<String>,
    last_event_at: Option<std::time::Instant>,
}

impl TypedLog {
    pub fn new(start_armed: bool) -> Self {
        Self {
            text: String::new(),
            armed: start_armed,
            last_injected: None,
            restore_token: None,
            last_event_at: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn armed(&self) -> bool {
        self.armed
    }

    pub fn set_last_accept(&mut self, inject: Option<String>, restore_token: Option<String>) {
        self.last_injected = inject;
        self.restore_token = restore_token;
    }

    pub fn take_last_accept(&mut self) -> Option<(String, String)> {
        let inject = self.last_injected.take()?;
        let restore_token = self.restore_token.take().unwrap_or_default();
        Some((inject, restore_token))
    }

    fn clear_last_accept(&mut self) {
        self.last_injected = None;
        self.restore_token = None;
    }

    pub fn toggle(&mut self, cfg: &CompletionKeyboardConfig) {
        if self.armed {
            self.armed = false;
        } else {
            self.arm(cfg);
        }
    }

    pub fn arm(&mut self, cfg: &CompletionKeyboardConfig) {
        self.armed = true;
        if cfg.clear_log_on_arm {
            self.text.clear();
            self.clear_last_accept();
        }
    }

    pub fn disarm(&mut self) {
        self.armed = false;
    }

    pub fn apply(&mut self, event: LogEvent, payload: &str, cfg: &CompletionKeyboardConfig) {
        self.last_event_at = Some(std::time::Instant::now());

        match event {
            LogEvent::Arrow if cfg.latch_off_on_arrow => {
                self.disarm();
                return;
            }
            LogEvent::Paste if cfg.latch_off_on_paste => {
                self.disarm();
                return;
            }
            LogEvent::CtrlAlt if cfg.ignore_ctrl_alt => {
                return;
            }
            _ => {}
        }

        if !self.armed {
            return;
        }

        match event {
            LogEvent::Char(ch) => self.push_str(&ch.to_string(), cfg),
            LogEvent::Text => self.push_str(payload, cfg),
            LogEvent::Backspace if cfg.track_backspace => self.pop_char(),
            LogEvent::Enter if cfg.clear_on_enter => {
                self.text.clear();
                self.clear_last_accept();
            }
            LogEvent::Arrow
            | LogEvent::Paste
            | LogEvent::CtrlAlt
            | LogEvent::Backspace
            | LogEvent::Enter => {}
        }
    }

    pub fn maybe_idle_reset(&mut self, cfg: &CompletionKeyboardConfig) {
        if cfg.idle_reset_ms == 0 {
            return;
        }
        let Some(t) = self.last_event_at else {
            return;
        };
        if t.elapsed().as_millis() as u64 >= cfg.idle_reset_ms {
            self.text.clear();
            self.clear_last_accept();
        }
    }

    fn push_str(&mut self, s: &str, cfg: &CompletionKeyboardConfig) {
        self.text.push_str(s);
        if self.text.len() > cfg.max_chars {
            let extra = self.text.len() - cfg.max_chars;
            let start = self
                .text
                .char_indices()
                .find(|(i, _)| *i >= extra)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.text.replace_range(0..start, "");
        }
    }

    fn pop_char(&mut self) {
        if let Some((i, _)) = self.text.char_indices().next_back() {
            self.text.truncate(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::completion::settings::CompletionKeyboardConfig;

    fn cfg() -> CompletionKeyboardConfig {
        CompletionKeyboardConfig::default()
    }

    #[test]
    fn append_backspace_enter() {
        let mut log = TypedLog::new(true);
        let c = cfg();
        log.apply(LogEvent::Char('h'), "", &c);
        log.apply(LogEvent::Char('i'), "", &c);
        assert_eq!(log.text(), "hi");
        log.apply(LogEvent::Backspace, "", &c);
        assert_eq!(log.text(), "h");
        log.apply(LogEvent::Enter, "", &c);
        assert_eq!(log.text(), "");
        assert!(log.armed());
    }

    #[test]
    fn ctrl_ignored() {
        let mut log = TypedLog::new(true);
        let c = cfg();
        log.apply(LogEvent::Char('a'), "", &c);
        log.apply(LogEvent::CtrlAlt, "c", &c);
        assert_eq!(log.text(), "a");
        assert!(log.armed());
    }

    #[test]
    fn arrow_disarms_and_stops_logging() {
        let mut log = TypedLog::new(true);
        let c = cfg();
        log.apply(LogEvent::Char('a'), "", &c);
        log.apply(LogEvent::Arrow, "", &c);
        assert!(!log.armed());
        log.apply(LogEvent::Char('b'), "", &c);
        assert_eq!(log.text(), "a");
    }

    #[test]
    fn paste_disarms() {
        let mut log = TypedLog::new(true);
        let c = cfg();
        log.apply(LogEvent::Paste, "", &c);
        assert!(!log.armed());
    }

    #[test]
    fn toggle_rearms_and_clears() {
        let mut log = TypedLog::new(true);
        let c = cfg();
        log.apply(LogEvent::Char('a'), "", &c);
        log.apply(LogEvent::Arrow, "", &c);
        log.toggle(&c);
        assert!(log.armed());
        assert_eq!(log.text(), "");
    }

    #[test]
    fn enter_does_not_rearm() {
        let mut log = TypedLog::new(true);
        let c = cfg();
        log.apply(LogEvent::Arrow, "", &c);
        log.apply(LogEvent::Enter, "", &c);
        assert!(!log.armed());
    }
}
