use std::io::{self, BufRead, Write};

use thiserror::Error;

use crate::canvas::Canvas;
use crate::network::{Message, ParseMessageError, Version};

pub const DEFAULT_PORT: &str = "45011";
const PROTOCOL_VERSION: Version = Version::new(1, 0);

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    ParseMessage(#[from] ParseMessageError),
}

pub trait Client: BufRead + Write + Sized {
    fn init_connection(&mut self) -> Result<Canvas, ProtocolError> {
        self.write_fmt(format_args!(
            "{}",
            Message::VersionReq {
                v: PROTOCOL_VERSION
            }
        ))
        .unwrap();

        let m = Message::from_reader(self).unwrap();
        match m {
            Message::VersionAck => (),
            _ => panic!(),
        }

        let m = Message::from_reader(self).unwrap();
        let canvas = match m {
            Message::CanvasSet { c } => c,
            _ => panic!(),
        };

        Ok(canvas)
    }

    fn send_char_update(&mut self, x: usize, y: usize, c: char) -> Result<(), ProtocolError> {
        self.write_fmt(format_args!("{}", Message::CharSet { x, y, c }))
            .unwrap();
        Ok(())
    }

    fn check_for_update(&mut self) -> Result<Option<(usize, usize, char)>, ProtocolError> {
        let m = Message::from_reader(self).unwrap();
        match m {
            Message::CharSet { x, y, c } => Ok(Some((x, y, c))),
            _ => panic!(),
        }
    }
}
