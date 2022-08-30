use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, Link, UID, Downgradable};
use crate::stream::{ByteStream, ByteStreamBody, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    ATEN_SWITCHSTREAM_DROP,
    ATEN_SWITCHSTREAM_UPPED_MISS,
    ATEN_SWITCHSTREAM_REGISTER_CALLBACK,
    ATEN_SWITCHSTREAM_UNREGISTER_CALLBACK,
    ATEN_SWITCHSTREAM_READ_TRIVIAL,
    ATEN_SWITCHSTREAM_READ,
    ATEN_SWITCHSTREAM_READ_DUMP,
    ATEN_SWITCHSTREAM_READ_TEXT,
    ATEN_SWITCHSTREAM_READ_FAIL);

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
    wrappee: ByteStream,
}

impl StreamBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.wrappee.read(buf)
    }
}

impl Stream {
    IMPL_STREAM!();

    pub fn new(disk: &Disk, wrappee: ByteStream) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_SWITCHSTREAM_CREATE {
            DISK: disk, STREAM: uid, WRAPPEE: wrappee,
        });
        let body = Rc::new(RefCell::new(StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
            wrappee: wrappee.clone(),
        }));
        let stream = Stream(Link {
            uid: uid,
            body: body.clone(),
        });
        stream.register_wrappee_callback(&wrappee);
        stream
    }

    pub fn switch(&self, wrappee: ByteStream) {
        self.register_wrappee_callback(&wrappee);
        self.0.body.borrow_mut().wrappee = wrappee;
    }
} // impl Stream
