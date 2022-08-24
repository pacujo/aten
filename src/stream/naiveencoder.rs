use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, Link, UID};
use crate::stream::{ByteStream, ByteStreamBody, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    ATEN_NAIVEENCODER_DROP,
    ATEN_NAIVEENCODER_UPPED_MISS,
    ATEN_NAIVEENCODER_REGISTER_CALLBACK,
    ATEN_NAIVEENCODER_UNREGISTER_CALLBACK,
    ATEN_NAIVEENCODER_READ_TRIVIAL,
    ATEN_NAIVEENCODER_READ,
    ATEN_NAIVEENCODER_READ_DUMP,
    ATEN_NAIVEENCODER_READ_TEXT,
    ATEN_NAIVEENCODER_READ_FAIL);

#[derive(Debug)]
enum State {
    Reading,
    Escaped,
    Exhausted,
    Terminated,
}

const BUF_SIZE: usize = 2000;

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
    wrappee: ByteStream,
    state: State,
    terminator: u8,
    escape: Option<u8>,
    buffer: [u8; BUF_SIZE],
    low: usize,
    high: usize,
}

impl StreamBody {
    fn encode(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut wi = 0;
        if matches!(self.state, State::Escaped) {
            assert!(self.low < self.high);
            buf[wi] = self.buffer[self.low];
            wi += 1;
            self.low += 1;
        }
        loop {
            if wi == buf.len() {
                self.state = State::Reading;
                return Ok(wi)
            }
            if self.low >= self.high {
                if wi > 0 {
                    self.state = State::Reading;
                    return Ok(wi)
                }
                return
                    match self.wrappee.read(&mut self.buffer) {
                        Ok(0) => {
                            self.state = State::Terminated;
                            buf[0] = self.terminator;
                            Ok(1)
                        }
                        Ok(count) => {
                            self.low = 0;
                            self.high = count;
                            self.encode(buf)
                        }
                        Err(err) => {
                            Err(err)
                        }
                    }
            }
            let next = self.buffer[self.low];
            if let Some(escape) = self.escape {
                if next == escape || next == self.terminator {
                    buf[wi] = escape;
                    wi += 1;
                    if wi == buf.len() {
                        self.state = State::Escaped;
                        return Ok(wi);
                    }
                }
            }
            buf[wi] = next;
            wi += 1;
            self.low += 1;
        }
    }

    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        match self.state {
            State::Reading | State::Escaped => {
                self.encode(buf)
            }
            State::Exhausted => {
                self.state = State::Terminated;
                buf[0] = self.terminator;
                Ok(1)
            }
            State::Terminated => {
                Ok(0)
            }
        }
    }
}

impl Stream {
    IMPL_STREAM!();

    pub fn new(disk: &Disk,
               wrappee: ByteStream,
               terminator: u8,
               escape: Option<u8>) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_NAIVEENCODER_CREATE {
            DISK: disk, STREAM: uid, WRAPPEE: wrappee,
            TERMINATOR: terminator, ESCAPE: r3::option(&escape),
        });
        let body = Rc::new(RefCell::new(StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
            wrappee: wrappee.clone(),
            state: State::Reading,
            terminator: terminator,
            escape: escape,
            buffer: [0; BUF_SIZE],
            low: 0,
            high: 0,
        }));
        let stream = Stream(Link {
            uid: uid,
            body: body.clone(),
        });
        stream.register_wrappee_callback(&wrappee);
        stream
    }
} // impl Stream
