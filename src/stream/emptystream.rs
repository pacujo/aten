use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Action, Disk, Link, UID, callback_to_string};
use crate::stream::{BaseStreamBody, ByteStreamBody};
use r3::TRACE;

DECLARE_STREAM!(EmptyStream, WeakEmptyStream, EmptyStreamBody);

#[derive(Debug)]
struct EmptyStreamBody {
    base: BaseStreamBody,
}

impl ByteStreamBody for EmptyStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        TRACE!(ATEN_EMPTYSTREAM_READ {
            STREAM: self, WANT: buf.len(), GOT: 0
        });
        Ok(0)
    }

    fn close(&mut self) {
        TRACE!(ATEN_EMPTYSTREAM_CLOSE { STREAM: self });
        self.base.close();
    }

    fn register(&mut self, callback: Option<Action>) {
        TRACE!(ATEN_EMPTYSTREAM_REGISTER {
            STREAM: self, CALLBACK: callback_to_string(&callback)
        });
        self.base.register(callback);
    }
} // impl ByteStreamBody for EmptyStreamBody

impl EmptyStream {
    IMPL_STREAM!(WeakEmptyStream);

    pub fn new(disk: &Disk) -> EmptyStream {
        let uid = UID::new();
        TRACE!(ATEN_EMPTYSTREAM_CREATE { DISK: disk, STREAM: uid });
        let body = EmptyStreamBody {
            base: BaseStreamBody::new(disk.downgrade(), uid),
        };
        EmptyStream(Link {
            uid: uid,
            body: Rc::new(RefCell::new(body)),
        })
    }
} // impl EmptyStream
