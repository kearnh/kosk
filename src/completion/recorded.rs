//! Suggestions captured on a tape. Playback serves these instead of a dictionary.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use super::backend::{Abort, Candidate, CompletionBackend, MatchKind, Source};
use super::context::CompletionContext;
use crate::controller::record::{RecordEvent, RecordedChip, Tape};

pub struct RecordedSuggestions {
    queues: Mutex<HashMap<String, VecDeque<Vec<Candidate>>>>,
}

impl RecordedSuggestions {
    fn from_chips(chips: &[RecordedChip]) -> Vec<Candidate> {
        chips
            .iter()
            .map(|chip| Candidate {
                text: chip.text.clone(),
                score: 0.0,
                source: if chip.current_word {
                    Source::CurrentWord
                } else {
                    Source::Dictionary
                },
                kind: MatchKind::ExactPrefix,
            })
            .collect()
    }
}

/// `Some` for tape version >= 2, including a tape with no suggestion events.
/// Older tapes stay on the live engine.
pub fn playback_backend(tape: &Tape) -> Option<Arc<dyn CompletionBackend>> {
    if tape.header.version < 2 {
        return None;
    }

    let mut queues: HashMap<String, VecDeque<Vec<Candidate>>> = HashMap::new();
    for ev in &tape.events {
        let RecordEvent::Suggestions { prefix, chips, .. } = ev else {
            continue;
        };
        queues
            .entry(prefix.clone())
            .or_default()
            .push_back(RecordedSuggestions::from_chips(chips));
    }

    Some(Arc::new(RecordedSuggestions {
        queues: Mutex::new(queues),
    }))
}

impl CompletionBackend for RecordedSuggestions {
    fn suggest(&self, ctx: &CompletionContext, abort: &Abort<'_>) -> Option<Vec<Candidate>> {
        if abort.stale() {
            return None;
        }

        let mut queues = self.queues.lock().unwrap();
        let list = queues
            .get_mut(&ctx.prefix)
            .and_then(|q| q.pop_front())
            .unwrap_or_default();
        Some(list)
    }

    fn knows_word(&self, _ctx: &CompletionContext) -> bool {
        true
    }

    fn writes_user_cache(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use super::*;
    use crate::controller::record::{MappingScales, TapeHeader};

    fn ctx(prefix: &str) -> CompletionContext {
        CompletionContext {
            prefix: prefix.into(),
            suffix: String::new(),
            token: String::new(),
            token_range: 0..0,
            prev_words: Vec::new(),
            max_results: 4,
            capitalize_sentence: false,
            neighbors: HashMap::new(),
            app_type: "other".into(),
        }
    }

    fn chip(text: &str, current_word: bool) -> RecordedChip {
        RecordedChip {
            text: text.into(),
            current_word,
        }
    }

    fn suggestions(prefix: &str, chips: Vec<RecordedChip>) -> RecordEvent {
        RecordEvent::Suggestions {
            t_us: 0,
            prefix: prefix.into(),
            chips,
        }
    }

    fn tape(version: u32, events: Vec<RecordEvent>) -> Tape {
        Tape {
            header: TapeHeader {
                version,
                current_layout: "main".into(),
                scales: MappingScales {
                    scale_x: 1.0,
                    scale_y: 1.0,
                    stick_scale_x: 1.0,
                    stick_scale_y: 1.0,
                },
                config_toml: (version >= 1).then(|| "event_debounce_ms = 1\n".into()),
                layouts: Vec::new(),
            },
            events,
        }
    }

    #[test]
    fn v1_tape_keeps_live_engine() {
        let tape = tape(
            1,
            vec![suggestions(
                "hel",
                vec![chip("hello", false), chip("hel", true)],
            )],
        );
        assert!(playback_backend(&tape).is_none());
    }

    #[test]
    fn v2_serves_lists_in_order_per_prefix() {
        let tape = tape(
            2,
            vec![
                suggestions("hel", vec![chip("hello", false), chip("hel", true)]),
                suggestions("hel", vec![chip("help", false)]),
                suggestions("hello you", vec![chip("your", false)]),
            ],
        );
        let backend = playback_backend(&tape).expect("v2 uses recorded suggestions");
        assert!(!backend.writes_user_cache());
        assert!(backend.knows_word(&ctx("hel")));

        let current = AtomicU64::new(2);
        let stale = Abort {
            mine: 1,
            current: &current,
        };
        assert!(backend.suggest(&ctx("hel"), &stale).is_none());

        let current = AtomicU64::new(1);
        let abort = Abort {
            mine: 1,
            current: &current,
        };
        let first = backend.suggest(&ctx("hel"), &abort).unwrap();
        assert_eq!(first[0].text, "hello");
        assert_eq!(first[0].source, Source::Dictionary);
        assert_eq!(first[1].text, "hel");
        assert_eq!(first[1].source, Source::CurrentWord);

        let second = backend.suggest(&ctx("hel"), &abort).unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].text, "help");

        assert!(backend.suggest(&ctx("hel"), &abort).unwrap().is_empty());
        assert!(backend.suggest(&ctx("other"), &abort).unwrap().is_empty());

        let spaced = backend.suggest(&ctx("hello you"), &abort).unwrap();
        assert_eq!(spaced[0].text, "your");
    }

    #[test]
    fn v2_with_no_suggestion_events_is_still_recorded() {
        let backend = playback_backend(&tape(2, Vec::new())).expect("empty v2");
        let current = AtomicU64::new(1);
        let abort = Abort {
            mine: 1,
            current: &current,
        };
        assert!(backend.suggest(&ctx("hel"), &abort).unwrap().is_empty());
        assert!(backend.knows_word(&ctx("hel")));
    }
}
