use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Action, Disk, Link, UID, callback_to_string};
use crate::stream::{ByteStreamBody, base};
use r3::TRACE;

DECLARE_STREAM!(Stream, WeakStream, StreamBody, ATEN_ZEROSTREAM_DROP);

#[derive(Debug)]
struct StreamBody {
    base: base::StreamBody,
}

impl ByteStreamBody for StreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Ok(n) = self.base.read(buf) {
            TRACE!(ATEN_ZEROSTREAM_READ_TRIVIAL {
                STREAM: self, WANT: buf.len()
            });
            return Ok(n);
        }
        for slot in &mut buf.iter_mut() {
            *slot = 0;
        }
        TRACE!(ATEN_ZEROSTREAM_READ {
            STREAM: self, WANT: buf.len(), GOT: buf.len()
        });
        Ok(buf.len())
    }

    fn register(&mut self, callback: Option<Action>) {
        TRACE!(ATEN_ZEROSTREAM_REGISTER {
            STREAM: self, CALLBACK: callback_to_string(&callback)
        });
        self.base.register(callback);
    }
} // impl ByteStreamBody for StreamBody 

impl Stream {
    IMPL_STREAM!(WeakStream);

    pub fn new(disk: &Disk) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_ZEROSTREAM_CREATE { DISK: disk, STREAM: uid });
        let body = StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid)
        };
        Stream(Link {
            uid: uid,
            body: Rc::new(RefCell::new(body)),
        })
    }
} // impl Stream
