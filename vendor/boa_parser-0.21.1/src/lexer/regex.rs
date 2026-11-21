//! Boa's lexing for ECMAScript regex literals.

use crate::lexer::{Cursor, Error, Token, TokenKind, Tokenizer};
use crate::source::ReadChar;
use bitflags::bitflags;
use boa_ast::PositionGroup;
use boa_interner::Interner;
use regress::Flags;
use std::fmt::{Display, Write};
use std::str::{self, FromStr};

const MAXIMUM_REGEX_FLAGS: usize = 8;

/// Regex literal lexing.
///
/// Lexes Division, Assigndiv or Regex literal.
///
/// Expects: Initial '/' to already be consumed by cursor.
///
/// More information:
///  - [ECMAScript reference][spec]
///  - [MDN documentation][mdn]
///
/// [spec]: https://tc39.es/ecma262/#sec-literals-regular-expression-literals
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Regular_Expressions
#[derive(Debug, Clone, Copy)]
pub(super) struct RegexLiteral {
    // If there is more cases than only `/=`
    // then use `Option<u8>` or (more correct) `Option<enum>`
    init_with_eq: bool,
}

impl RegexLiteral {
    /// `init_with_eq` is '=' after `/` already consumed?
    pub(super) fn new(init_with_eq: bool) -> Self {
        Self { init_with_eq }
    }
}

impl<R> Tokenizer<R> for RegexLiteral {
    fn lex(
        &mut self,
        cursor: &mut Cursor<R>,
        start_pos: PositionGroup,
        interner: &mut Interner,
    ) -> Result<Token, Error>
    where
        R: ReadChar,
    {
        let mut body = Vec::new();
        if self.init_with_eq {
            body.push(u32::from(b'='));
        }

        let mut is_class_char = false;

        // Lex RegularExpressionBody.
        loop {
            match cursor.next_char()? {
                None => {
                    // Abrupt end.
                    return Err(Error::syntax(
                        "abrupt end on regular expression",
                        cursor.pos(),
                    ));
                }
                Some(b) => {
                    match b {
                        // /
                        0x2F if !is_class_char => break, // RegularExpressionBody finished.
                        // [
                        0x5B => {
                            is_class_char = true;
                            body.push(b);
                        }
                        // ]
                        0x5D if is_class_char => {
                            is_class_char = false;
                            body.push(b);
                        }
                        // \n | \r | \u{2028} | \u{2029}
                        0xA | 0xD | 0x2028 | 0x2029 => {
                            // Not allowed in Regex literal.
                            return Err(Error::syntax(
                                "new lines are not allowed in regular expressions",
                                cursor.pos(),
                            ));
                        }
                        // \
                        0x5C => {
                            // Escape sequence
                            body.push(b);
                            if let Some(sc) = cursor.next_char()? {
                                match sc {
                                    // \n | \r | \u{2028} | \u{2029}
                                    0xA | 0xD | 0x2028 | 0x2029 => {
                                        // Not allowed in Regex literal.
                                        return Err(Error::syntax(
                                            "new lines are not allowed in regular expressions",
                                            cursor.pos(),
                                        ));
                                    }
                                    b => body.push(b),
                                }
                            } else {
                                // Abrupt end of regex.
                                return Err(Error::syntax(
                                    "abrupt end on regular expression",
                                    cursor.pos(),
                                ));
                            }
                        }
                        _ => body.push(b),
                    }
                }
            }
        }

        let mut flags: [u32; MAXIMUM_REGEX_FLAGS] = [0; MAXIMUM_REGEX_FLAGS];
        let n = cursor.take_array_alphabetic(&mut flags)?;
        if n > MAXIMUM_REGEX_FLAGS {
            // There can only be a maximum of 8 flags.
            return Err(Error::syntax(
                "Invalid regular expression: too many flags",
                start_pos,
            ));
        }
        let flags: RegExpFlags =
            RegExpFlags::try_from(&flags[..n]).map_err(|e| Error::syntax(e, start_pos))?;

        // We have a vague hint of the size of this vector in the best case scenario.
        let mut body_utf16 = Vec::with_capacity(body.len());

        // We convert the body to UTF-16 since it may contain code points that are not valid UTF-8.
        // We already know that the body is valid UTF-16. Casting is fine.
        #[allow(clippy::cast_possible_truncation)]
        for cp in &body {
            let cp = *cp;
            if cp <= 0xFFFF {
                body_utf16.push(cp as u16);
            } else {
                let cp = cp - 0x1_0000;
                let high = 0xD800 | ((cp >> 10) as u16);
                let low = 0xDC00 | ((cp as u16) & 0x3FF);
                body_utf16.push(high);
                body_utf16.push(low);
            }
        }

        // Most hot-path regexp literals in the suite are plain characters and simple
        // backslash sequences. Skip the full parser for those non-Unicode forms.
        if flags.intersects(RegExpFlags::UNICODE | RegExpFlags::UNICODE_SETS)
            || !is_simple_non_unicode_pattern(&body)
        {
            // Scriptw: the pattern parser (here and in the compiler) recurses
            // once per group and, with the `v` flag, per nested class; deeper
            // patterns are a SyntaxError rather than a native stack overflow.
            if pattern_nesting_exceeds(
                body.iter().copied(),
                flags.contains(RegExpFlags::UNICODE_SETS),
                MAX_PATTERN_NESTING_DEPTH,
            ) {
                return Err(Error::syntax(
                    format!(
                        "Invalid regular expression literal: groups and classes nest more than \
                         the implementation limit of {MAX_PATTERN_NESTING_DEPTH} levels"
                    ),
                    start_pos,
                ));
            }
            // Legacy ClassAtoms are UTF-16 units, including each half of a raw
            // astral character. Unicode modes parse those characters as scalars.
            // Keep the interned source unchanged and validate the selected grammar.
            let parsed = if flags.intersects(RegExpFlags::UNICODE | RegExpFlags::UNICODE_SETS) {
                regress::backends::try_parse(body.iter().copied(), flags.into())
            } else {
                regress::backends::try_parse(
                    body_utf16.iter().copied().map(u32::from),
                    flags.into(),
                )
            };
            drop(parsed.map_err(|error| {
                Error::syntax(
                    format!("Invalid regular expression literal: {error}"),
                    start_pos,
                )
            })?);
        }

        Ok(Token::new_by_position_group(
            TokenKind::regular_expression_literal(
                interner.get_or_intern(body_utf16.as_slice()),
                interner.get_or_intern(flags.to_string().as_str()),
            ),
            start_pos,
            cursor.pos_group(),
        ))
    }
}

/// Scriptw: the deepest nesting of groups (and, with the `v` flag, of class
/// set operands) a regular expression literal may have. V8 compiles about
/// 10,000 nested groups and reports a SyntaxError for 20,000.
pub const MAX_PATTERN_NESTING_DEPTH: usize = 1000;

/// Scriptw: whether the pattern `body` (its code points, or the bytes of its
/// UTF-8 or WTF-8 encoding, whose ASCII bytes are its ASCII code points) nests
/// groups (or `v`-mode classes) more than `limit` deep, scanning escapes and
/// classes the way the pattern grammar does. It only over-counts parentheses
/// inside a class in `v` mode, where a nested class starts with `[` and a
/// parenthesis is a syntax error anyway. Scriptw's own pattern compiler
/// applies the same scan to every pattern it is given.
pub fn pattern_nesting_exceeds(
    body: impl IntoIterator<Item = u32>,
    unicode_sets: bool,
    limit: usize,
) -> bool {
    let mut depth = 0usize;
    let mut classes = 0usize;
    let mut code_points = body.into_iter();
    while let Some(cp) = code_points.next() {
        match char::from_u32(cp) {
            Some('\\') => {
                code_points.next();
            }
            Some('[') if classes == 0 || unicode_sets => {
                classes += 1;
                depth += 1;
            }
            Some(']') if classes > 0 => {
                classes -= 1;
                depth = depth.saturating_sub(1);
            }
            Some('(') if classes == 0 => depth += 1,
            Some(')') if classes == 0 => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth > limit {
            return true;
        }
    }
    false
}

fn is_simple_non_unicode_pattern(body: &[u32]) -> bool {
    let mut i = 0;
    while i < body.len() {
        match body[i] {
            0x28 | 0x29 | 0x2A | 0x2B | 0x3F | 0x5B | 0x5D | 0x7B | 0x7D => return false,
            0x5C => {
                i += 1;
                if i >= body.len() {
                    return false;
                }
            }
            _ => {}
        }
        i += 1;
    }
    true
}

bitflags! {
    /// Flags of a regular expression.
    #[derive(Debug, Default, Copy, Clone)]
    pub struct RegExpFlags: u8 {
        /// Whether to test the regular expression against all possible matches in a string,
        /// or only against the first.
        const GLOBAL = 0b0000_0001;

        /// Whether to ignore case while attempting a match in a string.
        const IGNORE_CASE = 0b0000_0010;

        /// Whether or not to search in strings across multiple lines.
        const MULTILINE = 0b0000_0100;

        /// Whether `.` matches newlines or not.
        const DOT_ALL = 0b0000_1000;

        /// Whether or not Unicode features are enabled.
        const UNICODE = 0b0001_0000;

        /// Whether or not the search is sticky.
        const STICKY = 0b0010_0000;

        /// Whether the regular expression result exposes the start and end indices of
        /// captured substrings.
        const HAS_INDICES = 0b0100_0000;

        /// Whether or not UnicodeSets features are enabled.
        const UNICODE_SETS = 0b1000_0000;
    }
}

impl TryFrom<&[u32]> for RegExpFlags {
    type Error = String;

    fn try_from(value: &[u32]) -> Result<Self, Self::Error> {
        let mut flags = Self::default();
        for c in value {
            let c = char::from_u32(*c)
                .ok_or_else(|| format!("Invalid regular expression flag: {c}"))?;

            let new_flag = match c {
                'g' => Self::GLOBAL,
                'i' => Self::IGNORE_CASE,
                'm' => Self::MULTILINE,
                's' => Self::DOT_ALL,
                'u' => Self::UNICODE,
                'y' => Self::STICKY,
                'd' => Self::HAS_INDICES,
                'v' => Self::UNICODE_SETS,
                _ => return Err(format!("invalid regular expression flag {c}")),
            };

            if flags.contains(new_flag) {
                return Err(format!("repeated regular expression flag {c}"));
            }
            flags.insert(new_flag);
        }

        if flags.contains(Self::UNICODE) && flags.contains(Self::UNICODE_SETS) {
            return Err("cannot use both 'u' and 'v' flags".into());
        }

        Ok(flags)
    }
}

impl FromStr for RegExpFlags {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut flags = Self::default();
        for c in s.bytes() {
            let new_flag = match c {
                b'g' => Self::GLOBAL,
                b'i' => Self::IGNORE_CASE,
                b'm' => Self::MULTILINE,
                b's' => Self::DOT_ALL,
                b'u' => Self::UNICODE,
                b'y' => Self::STICKY,
                b'd' => Self::HAS_INDICES,
                b'v' => Self::UNICODE_SETS,
                _ => return Err(format!("invalid regular expression flag {}", char::from(c))),
            };

            if flags.contains(new_flag) {
                return Err(format!(
                    "repeated regular expression flag {}",
                    char::from(c)
                ));
            }
            flags.insert(new_flag);
        }

        if flags.contains(Self::UNICODE) && flags.contains(Self::UNICODE_SETS) {
            return Err("cannot use both 'u' and 'v' flags".into());
        }

        Ok(flags)
    }
}

impl Display for RegExpFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.contains(Self::HAS_INDICES) {
            f.write_char('d')?;
        }
        if self.contains(Self::GLOBAL) {
            f.write_char('g')?;
        }
        if self.contains(Self::IGNORE_CASE) {
            f.write_char('i')?;
        }
        if self.contains(Self::MULTILINE) {
            f.write_char('m')?;
        }
        if self.contains(Self::DOT_ALL) {
            f.write_char('s')?;
        }
        if self.contains(Self::UNICODE) {
            f.write_char('u')?;
        }
        if self.contains(Self::STICKY) {
            f.write_char('y')?;
        }
        if self.contains(Self::UNICODE_SETS) {
            f.write_char('v')?;
        }
        Ok(())
    }
}

impl From<RegExpFlags> for Flags {
    fn from(value: RegExpFlags) -> Self {
        Self {
            icase: value.contains(RegExpFlags::IGNORE_CASE),
            multiline: value.contains(RegExpFlags::MULTILINE),
            dot_all: value.contains(RegExpFlags::DOT_ALL),
            unicode: value.contains(RegExpFlags::UNICODE)
                || value.contains(RegExpFlags::UNICODE_SETS),
            unicode_sets: value.contains(RegExpFlags::UNICODE_SETS),
            ..Self::default()
        }
    }
}
