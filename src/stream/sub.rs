use std::rc::Rc;
use std::cell::RefCell;
//use std::io::{Result, Error};
use std::io::Result;

use crate::{Action, Disk, Link, UID};
//use crate::{again, is_again};
use crate::stream::{ByteStream, ByteStreamBody, base};
use r3::TRACE;

DECLARE_STREAM!(
    ATEN_SUBSTREAM_DROP,
    ATEN_SUBSTREAM_UPPED_MISS,
    ATEN_SUBSTREAM_REGISTER,
    ATEN_SUBSTREAM_READ_TRIVIAL,
    ATEN_SUBSTREAM_READ,
    ATEN_SUBSTREAM_READ_DUMP,
    ATEN_SUBSTREAM_READ_FAIL);

pub struct StreamBody {
    base: base::StreamBody,
    wrappee: ByteStream,
    begin: u128,
    end: Option<u128>,
    exhaust: bool,
    cursor: u128,
    notification: Option<Action>,
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
            if self.exhaust {
                loop {
                    if let Err(err) = self.wrappee.read(buf) {
                        return Err(err);
                    }
                }
            }
            Ok(0)
        } else {
            self.wrappee.read(buf)
        }
    }
}

impl std::fmt::Debug for StreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamBody")
            .field("base", &self.base)
            .field("wrappee", &self.wrappee)
            .field("begin", &self.begin)
            .field("end", &self.end)
            .field("exhaust", &self.exhaust)
            .field("cursor", &self.cursor)
            .field("notification", &self.notification.is_some())
            .finish()
    }
} // impl Debug for StreamBody 

impl Stream {
    IMPL_STREAM_WRAPPEE!();

    pub fn new(disk: &Disk,
               wrappee: &ByteStream,
               begin: u128,
               end: Option<u128>,
               exhaust: bool) -> Stream {
        // If exhaust is true, an EOF is not delivered before wrappee
        // is exhausted. Otherwise, EOF is delivered as soon as end is
        // reached, and wrappee can be further read by outside
        // parties.
        let uid = UID::new();
        TRACE!(ATEN_SUBSTREAM_CREATE {
            DISK: disk, STREAM: uid, WRAPPEE: wrappee,
            BEGIN: begin, END: r3::option(&end), EXHAUST: exhaust
        });
        let body = Rc::new(RefCell::new(StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
            wrappee: wrappee.clone(),
            begin: begin,
            end: end,
            exhaust: exhaust,
            cursor: 0,
            notification: None,
        }));
        let stream = Stream(Link {
            uid: uid,
            body: body.clone(),
        });
        // "tie the knot"
        let weak_stream = stream.downgrade();
        let action = Rc::new(move || {
            weak_stream.upped(|stream| {
                if let Some(action) = stream.get_callback() {
                    (action)();
                }
            });
        });
        body.borrow_mut().notification = Some(action);
        stream.register_wrappee_callback(wrappee);
        stream
    }
} // impl Stream
