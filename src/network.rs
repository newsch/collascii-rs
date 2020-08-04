//! Network protocol-related structures
use std::error::Error;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::num::ParseIntError;
use std::str::FromStr;

use crate::canvas::Canvas;

use std::io::{self, BufRead};

#[derive(Debug, PartialEq)]
pub enum ParseVersionError {
    NoMajor,
    NoMinor,
    ExtraStuff(String),
    MajorParseError(ParseIntError),
    MinorParseError(ParseIntError),
}

impl Display for ParseVersionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use ParseVersionError::*;
        match self {
            NoMajor => write!(f, "Empty string"),
            NoMinor => write!(f, "Cannot split version"),
            ExtraStuff(s) => write!(f, "Unexpected extra info: {:?}", s),
            MajorParseError(e) => write!(f, "Cannot parse major: {}", e),
            MinorParseError(e) => write!(f, "Cannot parse minor: {}", e),
        }
    }
}

impl Error for ParseVersionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        use ParseVersionError::*;
        match self {
            MajorParseError(e) => Some(e),
            MinorParseError(e) => Some(e),
            _ => None,
        }
    }
}

/// A major.minor version
/// ```
/// use collascii::network::Version;
/// assert_eq!("1.2".parse::<Version>(), Ok(Version::new(1,2)));
/// assert!(matches!("1".parse::<Version>(), Err(_)));
/// assert!(matches!(".1".parse::<Version>(), Err(_)));
/// assert!(matches!("foo".parse::<Version>(), Err(_)));
/// assert!(matches!("foo".parse::<Version>(), Err(_)));
/// ```
#[derive(Debug, PartialEq, Clone)]
pub struct Version {
    major: u8,
    minor: u8,
}

impl Version {
    pub const fn new(major: u8, minor: u8) -> Self {
        Self { major, minor }
    }
}

impl FromStr for Version {
    type Err = ParseVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use ParseVersionError::*;
        let mut parts = s.split('.');
        let major = parts.next().ok_or(NoMajor)?;
        let minor = parts.next().ok_or(NoMinor)?;
        if let Some(s) = parts.next() {
            return Err(ExtraStuff(s.to_string()));
        }
        let major = major.parse::<u8>().map_err(|e| MajorParseError(e))?;
        let minor = minor.parse::<u8>().map_err(|e| MinorParseError(e))?;

        Ok(Self { major, minor })
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// A message sent between instances to modify a shared canvas.
#[derive(Debug, PartialEq, Clone)]
pub enum Message {
    /// Set a character in the canvas
    SetChar { x: usize, y: usize, c: char },
    /// Replace the canvas
    CanvasUpdate { c: Canvas },
    // /// A new client has joined
    // ClientJoined,
    // /// A client has quit
    // ClientQuit,
    /// Request a protocol version to use
    VersionReq { v: Version },
    /// Acknowledge the version to use
    /// Sent in response to a Protocol
    VersionAck,
    /// Exit message
    Quit,
}

impl Message {
    /// Parse a readable buffer and try to build a message from it.
    pub fn from_reader<R>(source: &mut R) -> Result<Self, io::Error>
    where
        R: BufRead,
    {
        let mut line = String::new();
        let size = source.read_line(&mut line)?;
        let parse_error = |msg: &str| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Parse Error: {}: {:?}", msg, line.clone()),
            )
        };
        if line.len() == 0 {
            return Ok(Message::Quit);
        }
        // TODO: fix up the error handling here
        // TODO: fix the numbering here - vals vs prefix
        let vals: Vec<&str> = line.split_ascii_whitespace().collect(); // all of the items in the message, including the prefix
        if (vals.len() == 0) {
            return Err(parse_error("Line has no content"));
        }
        let prefix = vals[0];

        match prefix {
            // SetChar
            "s" => {
                if vals.len() != 4 {
                    return Err(parse_error(&format!(
                        "Expected 4 arguments for SetChar, got {}",
                        vals.len()
                    )));
                }
                let y: usize = vals[1]
                    .parse()
                    .map_err(|_| parse_error("Invalid y value"))?;
                let x: usize = vals[2]
                    .parse()
                    .map_err(|_| parse_error("Invalid x value"))?;
                let c: char = vals[3]
                    .parse()
                    .map_err(|_| parse_error("Invalid c value"))?;
                Ok(Message::SetChar { y, x, c })
            }
            // CanvasUpdate
            "cs" => {
                if vals.len() != 3 {
                    return Err(parse_error(&format!(
                        "Expected 4 arguments for CanvasUpdate, got {}",
                        vals.len()
                    )));
                }
                let height: usize = vals[1]
                    .parse()
                    .map_err(|_| parse_error("Invalid height value"))?;
                let width: usize = vals[2]
                    .parse()
                    .map_err(|_| parse_error("Invalid width value"))?;
                let mut canvas = Canvas::new(width, height);
                // load data into canvas
                let bytes_to_read = width * height;
                let mut buf = vec![0u8; bytes_to_read];
                source.read_exact(&mut buf);
                let buf = String::from_utf8(buf)
                    .map_err(|e| parse_error(&format!("Couldn't parse canvas contents: {}", e)))?;
                canvas.insert(&buf);
                Ok(Message::CanvasUpdate { c: canvas })
            }
            // VersionReq
            "v" => {
                if vals.len() == 1 {
                    return Err(parse_error(&format!(
                        "Expected 2 arguments for ProtocolVersionReq, got {}",
                        vals.len()
                    )));
                }
                let version = vals[1];
                let version = version
                    .parse::<Version>()
                    .map_err(|e| parse_error(&format!("Couldn't parse version: {}", e)))?;
                Ok(Message::VersionReq { v: version })
            }
            // VersionAck
            "vok" => Ok(Message::VersionAck),
            // Quit
            "q" => Ok(Message::Quit),
            _ => Err(parse_error("Unknown command")),
        }
    }
}

impl Into<String> for Message {
    fn into(self) -> String {
        format!("{}", self)
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Message::*;
        match self {
            SetChar { y, x, c } => writeln!(f, "s {} {} {}", y, x, c)?,
            CanvasUpdate { c } => {
                writeln!(f, "cs {} {} \n{}", c.height(), c.width(), c.serialize())?
            }
            VersionReq { v } => writeln!(f, "v {}", v)?,
            VersionAck => writeln!(f, "vok")?,
            Quit => writeln!(f, "q")?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::Canvas;
    use super::Message;
    use super::Version;

    /// Check parsing of individual messages
    #[test]
    fn parse() {
        use Message::*;
        // good test cases
        let mut c1 = Canvas::new(3, 2);
        c1.insert("X1234");
        let msg_test_cases = [
            // SetChar
            (SetChar { y: 3, x: 2, c: 'a' }, "s 3 2 a\n"),
            (SetChar { y: 1, x: 0, c: 'Z' }, "s 1 0 Z\n"),
            // Canvas
            (CanvasUpdate { c: c1 }, "cs 2 3\nX1234 "),
            // VersionReq
            (
                VersionReq {
                    v: Version::new(2, 3),
                },
                "v 2.3\n",
            ),
            // VersionReq
            (VersionAck, "vok\n"),
            // Quit
            (Quit, "q\n"),
        ];

        // parse them individually
        for (i, (expected, input)) in msg_test_cases.iter().enumerate() {
            let parsed = Message::from_reader(&mut input.as_bytes());
            eprintln!("{}: {:?} -> {:?}", i, input, expected);
            assert!(parsed.is_ok());
            assert_eq!(expected, &parsed.unwrap());
        }

        // Concat all messages into a big stream and read it
        let (expecteds, inputs): (Vec<_>, Vec<_>) = msg_test_cases.iter().cloned().unzip();
        let blob = inputs.iter().fold(String::new(), |mut acc, input| {
            acc.push_str(input);
            acc
        });
        let (expecteds, inputs): (Vec<_>, Vec<_>) = msg_test_cases.iter().cloned().unzip();
        eprintln!("blob: {:?}", blob);
        let mut reader = blob.as_bytes();
        for expected in expecteds.iter() {
            let parsed = Message::from_reader(&mut reader);
            eprintln!("parsed: {:?}", parsed);
            assert!(parsed.is_ok());
            assert_eq!(expected, &parsed.unwrap());
        }
    }
}
