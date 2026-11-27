//! Strict JSON source ownership, independent of the JavaScript parse goal.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonValue {
    Null,
    Boolean(bool),
    /// The actual binary64 result, including negative zero and overflow.
    Number(u64),
    /// JSON escapes preserve UTF-16 code units, including unpaired surrogates.
    String(Vec<u16>),
    Array(Vec<JsonValue>),
    /// Source order and duplicate names survive until native property creation.
    Object(Vec<(Vec<u16>, JsonValue)>),
}

#[derive(Clone)]
pub struct ParsedJson {
    source: String,
    value: Option<std::sync::Arc<JsonValue>>,
}
impl ParsedJson {
    pub fn source_text(&self) -> &str {
        &self.source
    }
    pub fn value(&self) -> &JsonValue {
        self.value.as_deref().expect("live parsed JSON owner")
    }
}
impl PartialEq for ParsedJson {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}
impl Eq for ParsedJson {}
impl std::fmt::Debug for ParsedJson {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedJson")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}
impl Drop for ParsedJson {
    fn drop(&mut self) {
        // All shared owners consume into_inner, so exactly one receives the
        // final tree even when different graph clones are retired concurrently.
        if let Some(value) = self.value.take().and_then(std::sync::Arc::into_inner) {
            discard_values(vec![value]);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonParseError {
    byte_offset: usize,
    message: &'static str,
}
impl JsonParseError {
    pub fn byte_offset(&self) -> usize {
        self.byte_offset
    }
    pub fn message(&self) -> &str {
        self.message
    }
}
impl std::fmt::Display for JsonParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON byte {}: {}", self.byte_offset, self.message)
    }
}
impl std::error::Error for JsonParseError {}

enum Frame {
    Array {
        values: Vec<JsonValue>,
        value_next: bool,
        allow_end: bool,
    },
    Object {
        values: Vec<(Vec<u16>, JsonValue)>,
        key: Option<Vec<u16>>,
        key_next: bool,
        allow_end: bool,
    },
}
#[derive(Default)]
struct Frames(Vec<Frame>);
impl Drop for Frames {
    fn drop(&mut self) {
        let mut values = Vec::new();
        for frame in self.0.drain(..) {
            match frame {
                Frame::Array {
                    values: children, ..
                } => values.extend(children),
                Frame::Object {
                    values: children, ..
                } => values.extend(children.into_iter().map(|(_, value)| value)),
            }
        }
        discard_values(values);
    }
}
// Neither a rejected partial tree nor an admitted owner needs recursive Drop.
fn discard_values(mut values: Vec<JsonValue>) {
    while let Some(value) = values.pop() {
        match value {
            JsonValue::Array(children) => values.extend(children),
            JsonValue::Object(children) => {
                values.extend(children.into_iter().map(|(_, value)| value))
            }
            JsonValue::Null
            | JsonValue::Boolean(_)
            | JsonValue::Number(_)
            | JsonValue::String(_) => {}
        }
    }
}

struct Parser<'a> {
    source: &'a str,
    offset: usize,
}
impl Parser<'_> {
    fn error(&self, message: &'static str) -> JsonParseError {
        JsonParseError {
            byte_offset: self.offset,
            message,
        }
    }
    fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.offset).copied()
    }
    fn take_if(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.offset += 1;
            true
        } else {
            false
        }
    }
    fn digits(&mut self) -> bool {
        let start = self.offset;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.offset += 1;
        }
        self.offset != start
    }
    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.offset += 1;
        }
    }
    fn expect(&mut self, byte: u8, message: &'static str) -> Result<(), JsonParseError> {
        if self.take_if(byte) {
            Ok(())
        } else {
            Err(self.error(message))
        }
    }
    fn literal(&mut self, literal: &str, value: JsonValue) -> Result<JsonValue, JsonParseError> {
        if !self.source[self.offset..].starts_with(literal) {
            return Err(self.error("invalid JSON literal"));
        }
        self.offset += literal.len();
        Ok(value)
    }
    fn hex_unit(&mut self) -> Result<u16, JsonParseError> {
        let mut unit = 0u16;
        for _ in 0..4 {
            let digit = match self.peek() {
                Some(byte @ b'0'..=b'9') => byte - b'0',
                Some(byte @ b'a'..=b'f') => byte - b'a' + 10,
                Some(byte @ b'A'..=b'F') => byte - b'A' + 10,
                _ => return Err(self.error("JSON Unicode escape requires four hex digits")),
            };
            unit = unit * 16 + u16::from(digit);
            self.offset += 1;
        }
        Ok(unit)
    }
    fn string(&mut self) -> Result<Vec<u16>, JsonParseError> {
        self.expect(b'"', "JSON object keys and strings require double quotes")?;
        let mut units = Vec::new();
        loop {
            match self.peek() {
                None => return Err(self.error("unterminated JSON string")),
                Some(b'"') => {
                    self.offset += 1;
                    return Ok(units);
                }
                Some(b'\\') => {
                    self.offset += 1;
                    let escaped = self
                        .peek()
                        .ok_or_else(|| self.error("unterminated JSON escape"))?;
                    if !matches!(
                        escaped,
                        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' | b'u'
                    ) {
                        return Err(self.error("invalid JSON string escape"));
                    }
                    self.offset += 1;
                    units.push(match escaped {
                        b'"' => u16::from(b'"'),
                        b'\\' => u16::from(b'\\'),
                        b'/' => u16::from(b'/'),
                        b'b' => 8,
                        b'f' => 12,
                        b'n' => 10,
                        b'r' => 13,
                        b't' => 9,
                        b'u' => self.hex_unit()?,
                        _ => unreachable!("validated closed JSON escape"),
                    });
                }
                Some(0..=31) => {
                    return Err(self.error("unescaped control character in JSON string"));
                }
                Some(_) => {
                    let character = self.source[self.offset..]
                        .chars()
                        .next()
                        .expect("peek at a UTF-8 boundary");
                    self.offset += character.len_utf8();
                    let mut encoded = [0u16; 2];
                    units.extend_from_slice(character.encode_utf16(&mut encoded));
                }
            }
        }
    }
    fn number(&mut self) -> Result<JsonValue, JsonParseError> {
        let start = self.offset;
        self.take_if(b'-');
        match self.peek() {
            Some(b'0') => {
                self.offset += 1;
            }
            Some(b'1'..=b'9') => {
                self.offset += 1;
                self.digits();
            }
            _ => return Err(self.error("JSON number requires an integer part")),
        }
        if self.take_if(b'.') {
            if !self.digits() {
                return Err(self.error("JSON fraction requires a digit"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.offset += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.offset += 1;
            }
            if !self.digits() {
                return Err(self.error("JSON exponent requires a digit"));
            }
        }
        let number = self.source[start..self.offset]
            .parse::<f64>()
            .map_err(|_| self.error("JSON decimal cannot be converted to binary64"))?;
        Ok(JsonValue::Number(number.to_bits()))
    }
    fn start_value(&mut self, frames: &mut Frames) -> Result<Option<JsonValue>, JsonParseError> {
        self.whitespace();
        match self.peek() {
            Some(b'[') => {
                self.offset += 1;
                frames.0.push(Frame::Array {
                    values: Vec::new(),
                    value_next: true,
                    allow_end: true,
                });
                Ok(None)
            }
            Some(b'{') => {
                self.offset += 1;
                frames.0.push(Frame::Object {
                    values: Vec::new(),
                    key: None,
                    key_next: true,
                    allow_end: true,
                });
                Ok(None)
            }
            Some(b'"') => self.string().map(JsonValue::String).map(Some),
            Some(b't') => self.literal("true", JsonValue::Boolean(true)).map(Some),
            Some(b'f') => self.literal("false", JsonValue::Boolean(false)).map(Some),
            Some(b'n') => self.literal("null", JsonValue::Null).map(Some),
            Some(b'-' | b'0'..=b'9') => self.number().map(Some),
            _ => Err(self.error("expected a JSON value")),
        }
    }
    fn value(&mut self) -> Result<JsonValue, JsonParseError> {
        let mut frames = Frames::default();
        let mut pending = self.start_value(&mut frames)?;
        loop {
            if let Some(value) = pending.take() {
                match frames.0.last_mut() {
                    None => {
                        self.whitespace();
                        if self.peek().is_some() {
                            discard_values(vec![value]);
                            return Err(self.error("trailing text after JSON value"));
                        }
                        return Ok(value);
                    }
                    Some(Frame::Array {
                        values, value_next, ..
                    }) => {
                        debug_assert!(*value_next);
                        values.push(value);
                        *value_next = false;
                    }
                    Some(Frame::Object {
                        values,
                        key,
                        key_next,
                        ..
                    }) => {
                        values.push((
                            key.take().expect("JSON value follows its parsed key"),
                            value,
                        ));
                        *key_next = false;
                    }
                }
                continue;
            }
            self.whitespace();
            let closing = match frames.0.last_mut().expect("open JSON container") {
                Frame::Array {
                    value_next,
                    allow_end,
                    ..
                } if *value_next => {
                    if *allow_end && self.peek() == Some(b']') {
                        true
                    } else {
                        pending = self.start_value(&mut frames)?;
                        continue;
                    }
                }
                Frame::Array {
                    value_next,
                    allow_end,
                    ..
                } => match self.peek() {
                    Some(b']') => true,
                    Some(b',') => {
                        self.offset += 1;
                        *value_next = true;
                        *allow_end = false;
                        false
                    }
                    _ => return Err(self.error("JSON array requires comma or closing bracket")),
                },
                Frame::Object {
                    key,
                    key_next,
                    allow_end,
                    ..
                } if *key_next => {
                    if *allow_end && self.peek() == Some(b'}') {
                        true
                    } else {
                        let name = self.string()?;
                        self.whitespace();
                        self.expect(b':', "JSON object key requires a colon")?;
                        *key = Some(name);
                        pending = self.start_value(&mut frames)?;
                        continue;
                    }
                }
                Frame::Object {
                    key_next,
                    allow_end,
                    ..
                } => match self.peek() {
                    Some(b'}') => true,
                    Some(b',') => {
                        self.offset += 1;
                        *key_next = true;
                        *allow_end = false;
                        false
                    }
                    _ => return Err(self.error("JSON object requires comma or closing brace")),
                },
            };
            if closing {
                self.offset += 1;
                pending = Some(match frames.0.pop().expect("one open JSON container") {
                    Frame::Array { values, .. } => JsonValue::Array(values),
                    Frame::Object { values, .. } => JsonValue::Object(values),
                });
            }
        }
    }
}

pub fn parse_json(source: String) -> Result<ParsedJson, JsonParseError> {
    parse_json_retaining_source(source).map_err(|(_, error)| error)
}

#[doc(hidden)]
pub fn parse_json_retaining_source(source: String) -> Result<ParsedJson, (String, JsonParseError)> {
    let value = match (Parser {
        source: &source,
        offset: 0,
    })
    .value()
    {
        Ok(value) => value,
        Err(error) => return Err((source, error)),
    };
    Ok(ParsedJson {
        source,
        value: Some(std::sync::Arc::new(value)),
    })
}

#[cfg(test)]
mod tests;
