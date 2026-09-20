//! `when` clauses for layout display and controller mappings: parse once, eval per tick.

use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;
use std::fmt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WhenContext {
    pub shift: bool,
    pub recording: bool,
    pub replay: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub suggestion_selected: bool,
    pub completion_active: bool,
    pub just_accepted: bool,
}

impl WhenContext {
    pub fn modifier(self) -> bool {
        self.shift || self.ctrl || self.alt
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    Modifier,
    ModifierShift,
    ModifierCtrl,
    ModifierAlt,
    SuggestionSelected,
    CompletionActive,
    JustAccepted,
    Recording,
    Replay,
}

impl Flag {
    fn from_ident(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "modifier" => Some(Self::Modifier),
            "modifier.shift" | "shift" => Some(Self::ModifierShift),
            "modifier.ctrl" | "ctrl" => Some(Self::ModifierCtrl),
            "modifier.alt" | "alt" => Some(Self::ModifierAlt),
            "suggestionselected" => Some(Self::SuggestionSelected),
            "completionactive" => Some(Self::CompletionActive),
            "justaccepted" => Some(Self::JustAccepted),
            "recording" => Some(Self::Recording),
            "replay" => Some(Self::Replay),
            _ => None,
        }
    }

    fn eval(self, ctx: &WhenContext) -> bool {
        match self {
            Self::Modifier => ctx.modifier(),
            Self::ModifierShift => ctx.shift,
            Self::ModifierCtrl => ctx.ctrl,
            Self::ModifierAlt => ctx.alt,
            Self::SuggestionSelected => ctx.suggestion_selected,
            Self::CompletionActive => ctx.completion_active,
            Self::JustAccepted => ctx.just_accepted,
            Self::Recording => ctx.recording,
            Self::Replay => ctx.replay,
        }
    }
}

const FLAG_NAMES: &str = "suggestionSelected, completionActive, justAccepted, modifier, modifier.shift, modifier.ctrl, modifier.alt, recording, replay";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhenExpr {
    Flag(Flag),
    Not(Box<WhenExpr>),
    And(Box<WhenExpr>, Box<WhenExpr>),
    Or(Box<WhenExpr>, Box<WhenExpr>),
}

impl WhenExpr {
    pub fn parse(src: &str) -> Result<Self, String> {
        let mut parser = Parser::new(src)?;
        let expr = parser.parse_or()?;
        parser.expect_eof()?;
        Ok(expr)
    }

    pub fn eval(&self, ctx: &WhenContext) -> bool {
        match self {
            Self::Flag(f) => f.eval(ctx),
            Self::Not(e) => !e.eval(ctx),
            Self::And(a, b) => a.eval(ctx) && b.eval(ctx),
            Self::Or(a, b) => a.eval(ctx) || b.eval(ctx),
        }
    }
}

impl<'de> Deserialize<'de> for WhenExpr {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct WhenVisitor;

        impl Visitor<'_> for WhenVisitor {
            type Value = WhenExpr;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a when-clause string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                WhenExpr::parse(v).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(WhenVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token<'a> {
    Ident(&'a str),
    Not,
    And,
    Or,
    LParen,
    RParen,
}

struct Parser<'a> {
    src: &'a str,
    tokens: Vec<(Token<'a>, usize)>,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Result<Self, String> {
        Ok(Self {
            src,
            tokens: tokenize(src)?,
            pos: 0,
        })
    }

    fn peek(&self) -> Option<Token<'a>> {
        self.tokens.get(self.pos).map(|(t, _)| *t)
    }

    fn bump(&mut self) -> Option<Token<'a>> {
        let t = self.peek()?;
        self.pos += 1;
        Some(t)
    }

    fn at(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map(|(_, i)| *i)
            .unwrap_or(self.src.len())
    }

    fn parse_or(&mut self) -> Result<WhenExpr, String> {
        let mut left = self.parse_and()?;
        while self.peek() == Some(Token::Or) {
            self.bump();
            let right = self.parse_and()?;
            left = WhenExpr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<WhenExpr, String> {
        let mut left = self.parse_not()?;
        while self.peek() == Some(Token::And) {
            self.bump();
            let right = self.parse_not()?;
            left = WhenExpr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<WhenExpr, String> {
        if self.peek() == Some(Token::Not) {
            self.bump();
            Ok(WhenExpr::Not(Box::new(self.parse_not()?)))
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<WhenExpr, String> {
        match self.bump() {
            Some(Token::Ident(id)) => {
                let flag = Flag::from_ident(id).ok_or_else(|| {
                    format!("unknown when-clause identifier '{id}' (expected {FLAG_NAMES})")
                })?;
                Ok(WhenExpr::Flag(flag))
            }
            Some(Token::LParen) => {
                let expr = self.parse_or()?;
                match self.bump() {
                    Some(Token::RParen) => Ok(expr),
                    _ => Err(format!("expected ')' at byte {}", self.at())),
                }
            }
            Some(other) => Err(format!("unexpected token {other:?} at byte {}", self.at())),
            None => Err("unexpected end of when-clause".to_string()),
        }
    }

    fn expect_eof(&self) -> Result<(), String> {
        if let Some((tok, i)) = self.tokens.get(self.pos) {
            Err(format!("unexpected token {tok:?} at byte {i}"))
        } else {
            Ok(())
        }
    }
}

fn ident_ok(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn tokenize(src: &str) -> Result<Vec<(Token<'_>, usize)>, String> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut tokens = Vec::new();
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'!' => {
                tokens.push((Token::Not, i));
                i += 1;
            }
            b'(' => {
                tokens.push((Token::LParen, i));
                i += 1;
            }
            b')' => {
                tokens.push((Token::RParen, i));
                i += 1;
            }
            b'&' => {
                if bytes.get(i + 1) != Some(&b'&') {
                    return Err(format!("expected '&&' at byte {i}"));
                }
                tokens.push((Token::And, i));
                i += 2;
            }
            b'|' => {
                if bytes.get(i + 1) != Some(&b'|') {
                    return Err(format!("expected '||' at byte {i}"));
                }
                tokens.push((Token::Or, i));
                i += 2;
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    if ident_ok(bytes[i]) {
                        i += 1;
                        continue;
                    }
                    if bytes[i] != b'.' {
                        break;
                    }
                    let next = bytes.get(i + 1).copied();
                    if next.is_none_or(|c| !ident_ok(c)) {
                        return Err(format!("expected identifier after '.' at byte {i}"));
                    }
                    i += 1;
                }
                tokens.push((Token::Ident(&src[start..i]), start));
            }
            c => {
                return Err(format!("unexpected character '{}' at byte {i}", c as char));
            }
        }
    }
    if tokens.is_empty() {
        return Err("empty when-clause".to_string());
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> WhenExpr {
        WhenExpr::parse(s).unwrap()
    }

    fn ctx(shift: bool, recording: bool, replay: bool, ctrl: bool, alt: bool) -> WhenContext {
        WhenContext {
            shift,
            recording,
            replay,
            ctrl,
            alt,
            ..WhenContext::default()
        }
    }

    #[test]
    fn parses_flags_case_insensitive() {
        assert_eq!(parse("recording"), WhenExpr::Flag(Flag::Recording));
        assert_eq!(parse("SHIFT"), WhenExpr::Flag(Flag::ModifierShift));
        assert_eq!(parse("Replay"), WhenExpr::Flag(Flag::Replay));
        assert_eq!(parse("modifier.shift"), WhenExpr::Flag(Flag::ModifierShift));
        assert_eq!(
            parse("suggestionSelected"),
            WhenExpr::Flag(Flag::SuggestionSelected)
        );
        assert_eq!(
            parse("completionActive"),
            WhenExpr::Flag(Flag::CompletionActive)
        );
        assert_eq!(parse("justAccepted"), WhenExpr::Flag(Flag::JustAccepted));
    }

    #[test]
    fn modifier_any_and_dotted() {
        let e = parse("modifier");
        assert!(e.eval(&ctx(true, false, false, false, false)));
        assert!(e.eval(&ctx(false, false, false, true, false)));
        assert!(!e.eval(&ctx(false, false, false, false, false)));

        let e = parse("modifier.ctrl");
        assert!(e.eval(&ctx(false, false, false, true, false)));
        assert!(!e.eval(&ctx(true, false, false, false, false)));
    }

    #[test]
    fn not_and_or_parens() {
        let e = parse("!shift");
        assert!(!e.eval(&ctx(true, false, false, false, false)));
        assert!(e.eval(&ctx(false, false, false, false, false)));

        let e = parse("recording && shift");
        assert!(e.eval(&ctx(true, true, false, false, false)));
        assert!(!e.eval(&ctx(false, true, false, false, false)));
        assert!(!e.eval(&ctx(true, false, false, false, false)));

        let e = parse("recording || replay");
        assert!(e.eval(&ctx(false, true, false, false, false)));
        assert!(e.eval(&ctx(false, false, true, false, false)));
        assert!(!e.eval(&ctx(false, false, false, false, false)));

        let e = parse("!(recording || replay)");
        assert!(!e.eval(&ctx(false, true, false, false, false)));
        assert!(e.eval(&ctx(false, false, false, false, false)));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        let e = parse("recording || shift && ctrl");
        assert!(e.eval(&ctx(false, true, false, false, false)));
        assert!(!e.eval(&ctx(true, false, false, false, false)));
        assert!(e.eval(&ctx(true, false, false, true, false)));
    }

    #[test]
    fn parens_override_precedence() {
        let e = parse("(recording || shift) && ctrl");
        assert!(!e.eval(&ctx(false, true, false, false, false)));
        assert!(e.eval(&ctx(false, true, false, true, false)));
    }

    #[test]
    fn unknown_ident_errors() {
        let err = WhenExpr::parse("foo").unwrap_err();
        assert!(
            err.contains("unknown when-clause identifier 'foo'"),
            "{err}"
        );
        assert!(WhenExpr::parse("suggestion").is_err());
        assert!(WhenExpr::parse("armed").is_err());
    }

    #[test]
    fn dotted_ident_errors() {
        assert!(WhenExpr::parse("modifier.").is_err());
        assert!(WhenExpr::parse(".shift").is_err());
        assert!(WhenExpr::parse("modifier..shift").is_err());
    }

    #[test]
    fn empty_and_trailing_errors() {
        assert!(WhenExpr::parse("").is_err());
        assert!(WhenExpr::parse("   ").is_err());
        assert!(WhenExpr::parse("shift shift").is_err());
        assert!(WhenExpr::parse("shift &&").is_err());
    }

    #[test]
    fn suggestion_selected_eval() {
        let e = parse("suggestionSelected");
        let on = WhenContext {
            suggestion_selected: true,
            ..WhenContext::default()
        };
        assert!(e.eval(&on));
        assert!(!e.eval(&WhenContext::default()));
        assert!(parse("!suggestionSelected").eval(&WhenContext::default()));
    }

    #[test]
    fn just_accepted_eval() {
        let e = parse("justAccepted");
        let on = WhenContext {
            just_accepted: true,
            ..WhenContext::default()
        };
        assert!(e.eval(&on));
        assert!(!e.eval(&WhenContext::default()));
    }
}
