use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, Link, UID, Downgradable};
use crate::stream::{ByteStream, ByteStreamBody, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    ATEN_SUBSTREAM_DROP,
    ATEN_SUBSTREAM_UPPED_MISS,
    ATEN_SUBSTREAM_REGISTER_CALLBACK,
    ATEN_SUBSTREAM_UNREGISTER_CALLBACK,
    ATEN_SUBSTREAM_READ_TRIVIAL,
    ATEN_SUBSTREAM_READ,
    ATEN_SUBSTREAM_READ_DUMP,
    ATEN_SUBSTREAM_READ_FAIL);

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
    wrappee: ByteStream,
    begin: u128,
    end: Option<u128>,
    cursor: u128,
}

impl StreamBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        let buf_size = buf.len() as u128;
        while self.cursor < self.begin {
            let room = std::cmp::min(
                buf_size, self.begin - self.cursor) as usize;
            match self.wrappee.read(&mut buf[..room]) {
                Ok(n) => {
                    self.cursor += n as u128;
                }
                Err(err) => {
                    return Err(err)
                }
            }
        }
        if let Some(end) = self.end {
            if self.cursor < end {
                let room = std::cmp::min(buf_size, end - self.cursor) as usize;
                match self.wrappee.read(&mut buf[..room]) {
                    Ok(n) => {
                        self.cursor += n as u128;
                        return Ok(n)
                    }
                    Err(err) => {
                        return Err(err)
                    }
                }
            }
            Ok(0)
        } else {
            let result = self.wrappee.read(buf);
            if matches!(result, Ok(0)) {
                self.end = Some(self.cursor); // mark exhaustion
            }
            result
        }
    }
}

impl Stream {
    IMPL_STREAM!();

    pub fn new(disk: &Disk,
               wrappee: ByteStream,
               begin: u128,
               end: Option<u128>) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_SUBSTREAM_CREATE {
            DISK: disk, STREAM: uid, WRAPPEE: wrappee,
            BEGIN: begin, END: r3::option(&end),
        });
        let body = Rc::new(RefCell::new(StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
            wrappee: wrappee.clone(),
            begin: begin,
            end: end,
            cursor: 0,
        }));
        let stream = Stream(Link {
            uid: uid,
            body: body.clone(),
        });
        stream.register_wrappee_callback(&wrappee);
        stream
    }

    pub fn remainder(&self) -> Option<ByteStream> {
        let body = self.0.body.borrow();
        if let Some(end) = body.end {
            if body.cursor >= end {
                return Some(body.wrappee.clone());
            }
        }
        None
    }
} // impl Stream
