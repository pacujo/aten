use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, Link, UID, Downgradable};
use crate::stream::{ByteStream, ByteStreamBody, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    ATEN_AVIDSTREAM_DROP,
    ATEN_AVIDSTREAM_UPPED_MISS,
    ATEN_AVIDSTREAM_REGISTER_CALLBACK,
    ATEN_AVIDSTREAM_UNREGISTER_CALLBACK,
    ATEN_AVIDSTREAM_READ_TRIVIAL,
    ATEN_AVIDSTREAM_READ,
    ATEN_AVIDSTREAM_READ_DUMP,
    ATEN_AVIDSTREAM_READ_FAIL);

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
    wrappee: ByteStream,
}

impl StreamBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut cursor = 0;
        while cursor < buf.len() {
            match self.wrappee.read(&mut buf[cursor..]) {
                Ok(0) => {
                    return Ok(cursor);
                }
                Ok(n) => {
                    cursor += n;
                }
                Err(err) => {
                    if cursor > 0 {
                        return Ok(cursor);
                    }
                    return Err(err);
                }
            }
        }
        Ok(cursor)
    }
}

impl Stream {
    IMPL_STREAM!();

    pub fn new(disk: &Disk, wrappee: ByteStream) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_AVIDSTREAM_CREATE {
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
} // impl Stream
