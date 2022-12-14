use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Result, Error};

use crate::{Disk, Link, UID, Downgradable};
use crate::stream::{BasicStream, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    Stream, WeakStream, StreamBody,
    ATEN_DRYSTREAM_DROP,
    ATEN_DRYSTREAM_UPPED_MISS,
    ATEN_DRYSTREAM_REGISTER_CALLBACK,
    ATEN_DRYSTREAM_UNREGISTER_CALLBACK,
    ATEN_DRYSTREAM_READ_TRIVIAL,
    ATEN_DRYSTREAM_READ,
    ATEN_DRYSTREAM_READ_DUMP,
    ATEN_DRYSTREAM_READ_FAIL);

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
}

impl StreamBody {
    fn read_nontrivial(&mut self, _buf: &mut [u8]) -> Result<usize> {
        Err(Error::from_raw_os_error(libc::EAGAIN))
    }
}

impl Stream {
    pub fn new(disk: &Disk) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_DRYSTREAM_CREATE { DISK: disk, STREAM: uid });
        let body = StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid)
        };
        Stream(Link {
            uid: uid,
            body: Rc::new(RefCell::new(body)),
        })
    }
} // impl Stream
