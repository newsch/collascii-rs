use std::io;

use thiserror::Error;

use crate::canvas::Canvas;
use crate::network::{Message, Messenger, ParseMessageError, Version};

pub const DEFAULT_PORT: &str = "45011";
const PROTOCOL_VERSION: Version = Version::new(1, 0);

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Parse(#[from] ParseMessageError),
    #[error("Received unexpected {msg} message: {reason}")]
    UnexpectedMessage { msg: Message, reason: &'static str },
    #[error("Protocol version is not supported: {0}")]
    UnsupportedVersion(Version),
    #[error("Client quit")]
    Quit,
}

pub trait Client: Messenger {
    fn init_connection(&mut self) -> Result<Canvas, ProtocolError> {
        use ProtocolError::*;

        self.send_msg(Message::VersionReq {
            v: PROTOCOL_VERSION,
        })?;

        let m = self.get_msg()?;
        match m {
            Message::VersionAck => (),
            msg => {
                return Err(UnexpectedMessage {
                    msg,
                    reason: "Expected VersionAck",
                })
            }
        }

        let m = self.get_msg()?;
        let canvas = match m {
            Message::CanvasSet { c } => c,
            msg => {
                return Err(UnexpectedMessage {
                    msg,
                    reason: "Expected CanvasSet",
                })
            }
        };

        Ok(canvas)
    }

    fn send_char_update(&mut self, x: usize, y: usize, c: char) -> Result<(), io::Error> {
        self.send_msg(Message::CharSet { x, y, c })
    }

    fn check_for_update(&mut self) -> Result<(usize, usize, char), ProtocolError> {
        use ProtocolError::UnexpectedMessage;

        match self.get_msg()? {
            Message::CharSet { x, y, c } => Ok((x, y, c)),
            msg => Err(UnexpectedMessage {
                msg,
                reason: "Expected CharSet",
            }),
        }
    }
}

pub trait Server: Messenger {
    fn get_canvas(&self) -> Canvas;

    fn init_connection(&mut self) -> Result<(), ProtocolError> {
        use Message::*;
        use ProtocolError::*;

        // version negotiation
        let m = self.get_msg()?;
        let version = match m {
            VersionReq { v } => v,
            msg => {
                return Err(UnexpectedMessage {
                    msg,
                    reason: "Expected VersionReq",
                })
            }
        };
        if version != PROTOCOL_VERSION {
            return Err(UnsupportedVersion(version));
        }
        self.send_msg(VersionAck)?;

        // send canvas
        self.send_msg(CanvasSet {
            c: self.get_canvas(),
        })?;

        Ok(())
    }

    fn send_char_update(&mut self, x: usize, y: usize, c: char) -> Result<(), io::Error> {
        self.send_msg(Message::CharSet { x, y, c })
    }

    fn check_for_update(&mut self) -> Result<(usize, usize, char), ProtocolError> {
        use Message::*;
        use ParseMessageError::UnknownPrefix;

        loop {
            match self.get_msg() {
                // ignore unrecognized messages from client
                Err(UnknownPrefix { .. }) => continue,
                Err(e) => break Err(e.into()),
                Ok(CharSet { x, y, c }) => break Ok((x, y, c)),
                Ok(Quit) => break Err(ProtocolError::Quit),
                Ok(msg) => {
                    break Err(ProtocolError::UnexpectedMessage {
                        msg,
                        reason: "Expected CharSet",
                    })
                }
            }
        }
    }
}
