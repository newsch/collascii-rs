use std::io::{BufRead, Write};

use crate::canvas::Canvas;
use crate::network::{Message, Version};

const PROTOCOL_VERSION: Version = Version::new(1, 0);

pub enum ProtocolError {}

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
            Message::CanvasSend { c } => c,
            _ => panic!(),
        };

        Ok(canvas)
    }

    fn send_char_update(&mut self, x: usize, y: usize, c: char) -> Result<(), ProtocolError> {
        self.write_fmt(format_args!("{}", Message::SetChar { x, y, c }))
            .unwrap();
        Ok(())
    }

    fn check_for_update(&mut self) -> Result<Option<(usize, usize, char)>, ProtocolError> {
        let m = Message::from_reader(self).unwrap();
        match m {
            Message::SetChar { x, y, c } => Ok(Some((x, y, c))),
            _ => panic!(),
        }
    }
}
