use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, Link, UID, Downgradable};
use crate::stream::{BasicStream, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    Stream, WeakStream, StreamBody,
    ATEN_ZEROSTREAM_DROP,
    ATEN_ZEROSTREAM_UPPED_MISS,
    ATEN_ZEROSTREAM_REGISTER_CALLBACK,
    ATEN_ZEROSTREAM_UNREGISTER_CALLBACK,
    ATEN_ZEROSTREAM_READ_TRIVIAL,
    ATEN_ZEROSTREAM_READ,
    ATEN_ZEROSTREAM_READ_DUMP,
    ATEN_ZEROSTREAM_READ_FAIL);

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
}

impl StreamBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        for slot in &mut buf.iter_mut() {
            *slot = 0;
        }
        Ok(buf.len())
    }
}

impl Stream {
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
