use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Action, Disk, Link, UID, callback_to_string};
use crate::stream::{ByteStreamBody, base};
use r3::TRACE;

DECLARE_STREAM!(ATEN_ZEROSTREAM_DROP, ATEN_ZEROSTREAM_UPPED_MISS);

#[derive(Debug)]
struct StreamBody {
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

IMPL_STREAM_BODY!(
    ATEN_ZEROSTREAM_REGISTER,
    ATEN_ZEROSTREAM_READ_TRIVIAL,
    ATEN_ZEROSTREAM_READ,
    ATEN_ZEROSTREAM_READ_DUMP,
    ATEN_ZEROSTREAM_READ_FAIL);

impl Stream {
    IMPL_STREAM!();

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
