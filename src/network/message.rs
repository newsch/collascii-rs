//! Network protocol-related structures
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io::{self, BufRead};
use std::num::ParseIntError;
use std::str::FromStr;

use crate::canvas::Canvas;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ParseVersionError {
    #[error("No major version found")]
    NoMajor,
    #[error("No minor version found")]
    NoMinor,
    #[error("Unexpected extra content: {0:?}")]
    ExtraStuff(String),
    #[error("Cannot parse major version")]
    MajorParseError(#[source] ParseIntError),
    #[error("Cannot parse minor version")]
    MinorParseError(#[source] ParseIntError),
}

/// A major.minor version
/// ```
/// use collascii::network::Version;
/// assert_eq!("1.2".parse::<Version>(), Ok(Version::new(1,2)));
/// assert!("1".parse::<Version>().is_err());
/// assert!(".1".parse::<Version>().is_err());
/// assert!("foo".parse::<Version>().is_err());
/// assert!("foo".parse::<Version>().is_err());
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

#[derive(Error, Debug)]
pub enum ParseMessageError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Expected {exp} for {msg}, found {found}")]
    ParamCount {
        msg: &'static str,
        exp: usize,
        found: usize,
    },
    #[error("Invalid value for {msg} param {param}: {val:?}")]
    InvalidParam {
        msg: &'static str,
        param: &'static str,
        val: String,
    },
    #[error("Message is not formatted correctly: {0:?}")]
    FormatError(String),
    #[error("Unknown prefix: {0:?}")]
    UnknownPrefix(String),
}

/// A message sent between instances to modify a shared canvas.
///
/// To parse a message from a text/bytes source, use [`Message::from_reader`].
/// Because byte arrays implement [`std::io::BufRead`], you can use them and strings directly:
/// ```
/// use collascii::network::Message;
/// let source = "s 2 1 A\n";
/// let msg = Message::from_reader(&mut source.as_bytes()).unwrap();
/// assert_eq!(Message::CharSet{ x: 1, y: 2, c: 'A' }, msg);
/// ```
///
/// The current canonical way to create a message is to `write_fmt!(format_args!("{}", msg))` it.
///
/// # The Network Protocol
///
/// This is meant to be the most canonical definition of the network protocol spec.
///
/// The initial version is informally defined by the C code of the [original collascii](https://github.com/olin/collascii), which this is meant to be backwards-compatible with.
///
/// To date, there are two version's of the protocol
/// - an unnamed one that encompasses everything in 1.0 except for version negotiation, used by the original collascii
/// - `1.0`: the protocol defined by this code and the loose spec below
///
/// ## Messages
///
/// - Messages are sent between clients and servers over TCP connections.
/// - Messages are ascii text.
/// - All messages end with a single newline character (`'\n'`).
/// - [Some messages](Message::CanvasSet) contain multiple newline characters.
/// - The first line of all messages should be should be enough to distinguish them and prepare to parse any remaining data.
/// - The first line of messages are no more than 64 characters long, including the newline.
/// - To remain forwards-compatible, servers should silently ignore messages with prefixes they do not recognize.
/// - Clients may fail on unrecognized messages. Updates to the protocol that require breaking changes to client behavior will increment the protocol version.
///
/// Messages generally take the form:
///
/// `"<prefix> [<param>]...\n"`
///
/// where
/// - The `<prefix>` is a short sequence of non-whitespace characters that vaguely represents the purpose of the message.
/// - A `<param>` is a sequence of non-whitespace characters that holds some type of data for the message.
/// - The `<prefix>` and any `<param>`s are separated from each other by a single space (` `).
///
/// For example, the [`Message::CharSet`] that sets the character at (1, 2) to `'A'` looks like `"s 2 1 A\n"`.
///
/// The `1.0` protocol looks like this:
/// 1. Client opens TCP connection to server
/// 2. Client sends a [`Message::VersionReq`] to server with it's expected protocol version.
/// 3.
///     - if server _does not_ support the requested protocol version, it **closes the connection**.
///     - if server _does_ support the requested protocol version, it sends a [`Message::VersionAck`].
/// 4. The server sends a [`Message::CanvasSet`] with the current contents.
/// 5. From here on out:
///     - server sends a [`Message::CharSet`] whenever a character is changed by another client
///     - client sends a [`Message::CharSet`] to change a character on the server.
/// 6. Client sends a [`Message::Quit`] and closes the connection.
///
/// When the connection is closed due to an error, the closing party may write a message explaining the reason why before closing.
#[non_exhaustive]
#[derive(Debug, PartialEq, Clone)]
pub enum Message {
    /// Set a single character in the canvas
    ///
    /// Sent from a client _or_ from the server once communication is established.
    /// A client sends it to change a character on the server's canvas, and the server sends it to the client when updated by another client.
    ///
    /// **Text format**: `"s <ypos> <xpos> <character>\n"`
    ///
    /// **Note**: if the character in question is space (`' '`), then the message will end with two spaces and a newline (`"...<xpos>  \n"`).
    CharSet { x: usize, y: usize, c: char },

    /// Replace the canvas
    ///
    /// Sent from the server to a client after negotiating versions.
    ///
    /// **Text format**: `"cs <width> <height>\n<canvasdata>\n"`
    ///
    /// where
    /// - `<canvasdata>` is each row of the canvas concatenated together starting with the top row (`y = 0`), as outputted by [`Canvas::serialize`].
    ///
    /// NOTE: `<canvasdata>` will always be `width * height* characters long.
    CanvasSet { c: Canvas },

    /// Request a protocol version to use
    ///
    /// **Text format**: `"v <version>...\n"`
    ///
    /// where
    /// - `version` is of the form `<major>.<minor>`, where `<major>` and `<minor>` are positive integers.
    ///
    /// NOTE: Multiple versions in the request is reserved for future protocol versions.
    /// Implementations for 1.0 should check only the first parameter and not check if more exist.
    VersionReq { v: Version },

    /// Acknowledge the version to use
    ///
    /// Sent from the server to a client in response to a [`Message::VersionReq`].
    ///
    /// **Text format**: `"vok [<version>]\n"`
    ///
    /// NOTE: Returning a version in the acknowledgement is reserved for future protocol versions.
    /// Implementations for 1.0 should not check if parameters exist or not.
    VersionAck,

    /// Graceful exit message
    ///
    /// Sent from a client to a server before closing the connection.
    ///
    /// **Text format**: `"q\n"`
    Quit,
}

impl Message {
    /// Parse a readable buffer and try to build a message from it.
    pub fn from_reader<R>(source: &mut R) -> Result<Self, ParseMessageError>
    where
        R: BufRead,
    {
        use ParseMessageError::*;

        let mut line = String::new();
        let _size = source.read_line(&mut line)?;
        if line.len() == 0 {
            return Err(FormatError(line.to_owned()));
        }
        let line = line
            .strip_suffix('\n')
            .ok_or(FormatError(line.to_owned()))?;
        let vals: Vec<&str> = line.split(' ').collect(); // all of the items in the message, including the prefix
        if vals.len() == 0 {
            return Err(FormatError(line.to_owned()));
        }
        let prefix = vals[0];
        let params = &vals[1..];
        match prefix {
            // CharSet
            "s" => {
                let msg = "Charset";
                let exp = 3;
                if params.len() < exp {
                    return Err(ParamCount {
                        msg,
                        exp,
                        found: params.len(),
                    });
                }

                let y: usize = params[0].parse().map_err(|_| InvalidParam {
                    msg,
                    param: "y",
                    val: params[0].to_owned(),
                })?;
                let x: usize = params[1].parse().map_err(|_| InvalidParam {
                    msg,
                    param: "x",
                    val: params[1].to_owned(),
                })?;
                let c: char = match (params[2], params.get(3)) {
                    ("", Some(&"")) => " ",
                    (_c, None) => _c,
                    (a, Some(b)) => {
                        return Err(InvalidParam {
                            msg,
                            param: "c",
                            val: format!("{} {}", a, b),
                        })
                    }
                }
                .parse()
                .map_err(|_| InvalidParam {
                    msg,
                    param: "c",
                    val: params[2].to_owned(),
                })?;
                if c != ' ' && c.is_ascii_whitespace() {
                    return Err(InvalidParam {
                        msg,
                        param: "c",
                        val: params[2].to_owned(),
                    });
                }
                Ok(Message::CharSet { y, x, c })
            }
            // CanvasSet
            "cs" => {
                let msg = "CanvasSet";
                let exp = 2;
                if params.len() != exp {
                    return Err(ParamCount {
                        msg,
                        exp,
                        found: params.len(),
                    });
                }
                let height: usize = params[0].parse().map_err(|_| InvalidParam {
                    msg,
                    param: "height",
                    val: params[0].to_owned(),
                })?;
                let width: usize = params[1].parse().map_err(|_| InvalidParam {
                    msg,
                    param: "width",
                    val: params[1].to_owned(),
                })?;
                let mut canvas = Canvas::new(width, height);
                // load data into canvas
                // all characters for canvas plus newline
                let bytes_to_read = width * height + 1;
                let mut buf = String::with_capacity(bytes_to_read);
                source.read_line(&mut buf)?;
                // this won't error out if more characters are read than can fill the canvas - any extra data will be dropped
                canvas.insert(&buf);
                Ok(Message::CanvasSet { c: canvas })
            }
            // VersionReq
            "v" => {
                let msg = "VersionReq";
                let exp = 1;
                if params.len() < exp {
                    return Err(ParamCount {
                        msg,
                        exp,
                        found: params.len(),
                    });
                }
                let version = params[0];
                let version = version.parse::<Version>().map_err(|_e| InvalidParam {
                    msg,
                    param: "version",
                    val: params[0].to_owned(),
                })?;
                Ok(Message::VersionReq { v: version })
            }
            // VersionAck
            "vok" => Ok(Message::VersionAck),
            // Quit
            "q" => Ok(Message::Quit),
            p => Err(UnknownPrefix(p.to_string())),
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
            CharSet { y, x, c } => writeln!(f, "s {} {} {}", y, x, c)?,
            CanvasSet { c } => writeln!(f, "cs {} {}\n{}", c.height(), c.width(), c.serialize())?,
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
    fn parse_good() {
        use Message::*;
        // good test cases
        let mut c1 = Canvas::new(3, 2);
        c1.insert("X1234");
        let msg_test_cases = [
            // CharSet
            (CharSet { y: 3, x: 2, c: 'a' }, "s 3 2 a\n"),
            (CharSet { y: 1, x: 0, c: 'Z' }, "s 1 0 Z\n"),
            (CharSet { y: 1, x: 0, c: ' ' }, "s 1 0  \n"),
            // Canvas
            (CanvasSet { c: c1 }, "cs 2 3\nX1234 \n"),
            // VersionReq
            (
                VersionReq {
                    v: Version::new(1, 0),
                },
                "v 1.0\n",
            ),
            (
                VersionReq {
                    v: Version::new(1, 0),
                },
                "v 1.0 1.1 1.2\n",
            ),
            // VersionAck
            (VersionAck, "vok\n"),
            (VersionAck, "vok 1.1\n"),
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
        let (_expecteds, inputs): (Vec<_>, Vec<_>) = msg_test_cases.iter().cloned().unzip();
        let blob = inputs.iter().fold(String::new(), |mut acc, input| {
            acc.push_str(input);
            acc
        });
        let (expecteds, _inputs): (Vec<_>, Vec<_>) = msg_test_cases.iter().cloned().unzip();
        eprintln!("blob: {:?}", blob);
        let mut reader = blob.as_bytes();
        for expected in expecteds.iter() {
            let parsed = Message::from_reader(&mut reader);
            eprintln!("parsed: {:?}", parsed);
            assert!(parsed.is_ok());
            assert_eq!(expected, &parsed.unwrap());
        }
    }

    #[test]
    fn parse_bad() {
        let bad_cases = [
            ("s 1 0 \n", "CharSet: whitespace but no character"),
            ("s 1 0  f\n", "CharSet: two spaces before character"),
            ("s 1 0 \t\n", "CharSet: tab character"),
            ("s 1 0 f\r", "return character only"),
            ("s 1 0 f\r\n", "return and newline characters"),
            ("s 1 0 f", "no newline"),
        ];
        for (case, description) in bad_cases.iter() {
            let result = Message::from_reader(&mut case.as_bytes());
            assert!(result.is_err(), *description);
        }
    }
}
