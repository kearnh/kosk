//! Layout `when` clauses: parse once at load, eval the AST at draw.

use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;
use std::fmt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisplayContext {
    pub shift: bool,
    pub recording: bool,
    pub replay: bool,
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    Shift,
    Recording,
    Replay,
    Ctrl,
    Alt,
}

impl Flag {
    fn from_ident(s: &str) -> Option<Self> {
        if s.eq_ignore_ascii_case("shift") {
            Some(Self::Shift)
        } else if s.eq_ignore_ascii_case("recording") {
            Some(Self::Recording)
        } else if s.eq_ignore_ascii_case("replay") {
            Some(Self::Replay)
        } else if s.eq_ignore_ascii_case("ctrl") {
            Some(Self::Ctrl)
        } else if s.eq_ignore_ascii_case("alt") {
            Some(Self::Alt)
        } else {
            None
        }
    }

    fn eval(self, ctx: &DisplayContext) -> bool {
        match self {
            Self::Shift => ctx.shift,
            Self::Recording => ctx.recording,
            Self::Replay => ctx.replay,
            Self::Ctrl => ctx.ctrl,
            Self::Alt => ctx.alt,
        }
    }
}

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

    pub fn eval(&self, ctx: &DisplayContext) -> bool {
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
                    format!("unknown when-clause identifier '{id}' (expected shift, recording, replay, ctrl, or alt)")
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
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
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

    fn ctx(shift: bool, recording: bool, replay: bool, ctrl: bool, alt: bool) -> DisplayContext {
        DisplayContext {
            shift,
            recording,
            replay,
            ctrl,
            alt,
        }
    }

    #[test]
    fn parses_flags_case_insensitive() {
        assert_eq!(parse("recording"), WhenExpr::Flag(Flag::Recording));
        assert_eq!(parse("SHIFT"), WhenExpr::Flag(Flag::Shift));
        assert_eq!(parse("Replay"), WhenExpr::Flag(Flag::Replay));
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
        // a || b && c  ==  a || (b && c)
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
    }

    #[test]
    fn empty_and_trailing_errors() {
        assert!(WhenExpr::parse("").is_err());
        assert!(WhenExpr::parse("   ").is_err());
        assert!(WhenExpr::parse("shift shift").is_err());
        assert!(WhenExpr::parse("shift &&").is_err());
    }
}
