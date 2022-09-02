use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, Link, UID, Downgradable};
use crate::stream::{ByteStreamBody, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    ATEN_EMPTYSTREAM_DROP,
    ATEN_EMPTYSTREAM_UPPED_MISS,
    ATEN_EMPTYSTREAM_REGISTER_CALLBACK,
    ATEN_EMPTYSTREAM_UNREGISTER_CALLBACK,
    ATEN_EMPTYSTREAM_READ_TRIVIAL,
    ATEN_EMPTYSTREAM_READ,
    ATEN_EMPTYSTREAM_READ_DUMP,
    ATEN_EMPTYSTREAM_READ_FAIL);

#[derive(Debug)]
struct StreamBody {
    base: base::StreamBody,
}

impl StreamBody {
    fn read_nontrivial(&mut self, _buf: &mut [u8]) -> Result<usize> {
        Ok(0)
    }
}

impl Stream {
    IMPL_STREAM!();

    pub fn new(disk: &Disk) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_EMPTYSTREAM_CREATE { DISK: disk, STREAM: uid });
        let body = StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
        };
        Stream(Link {
            uid: uid,
            body: Rc::new(RefCell::new(body)),
        })
    }
} // impl Stream
