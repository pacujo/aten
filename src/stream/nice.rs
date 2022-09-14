use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, Link, UID, Downgradable, error};
use crate::stream::{ByteStream, BasicStream, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    Stream, WeakStream, StreamBody,
    ATEN_NICESTREAM_DROP,
    ATEN_NICESTREAM_UPPED_MISS,
    ATEN_NICESTREAM_REGISTER_CALLBACK,
    ATEN_NICESTREAM_UNREGISTER_CALLBACK,
    ATEN_NICESTREAM_READ_TRIVIAL,
    ATEN_NICESTREAM_READ,
    ATEN_NICESTREAM_READ_DUMP,
    ATEN_NICESTREAM_READ_FAIL);

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
    wrappee: ByteStream,
    max_burst: usize,
    cursor: usize,
}

impl StreamBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.cursor >= self.max_burst {
            TRACE!(ATEN_NICESTREAM_BACK_OFF { STREAM: self });
            self.cursor = 0;
            self.base.invoke_callback();
            return Err(error::again());
        }
        match self.wrappee.read(buf) {
            Ok(n) => {
                self.cursor += n;
                Ok(n)
            }
            Err(err) => {
                self.cursor = 0;
                Err(err)
            }
        }
    }
}

impl Stream {
    pub fn new(disk: &Disk, wrappee: ByteStream, max_burst: usize) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_NICESTREAM_CREATE {
            DISK: disk, STREAM: uid, WRAPPEE: wrappee, MAX_BURST: max_burst,
        });
        let body = Rc::new(RefCell::new(StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
            wrappee: wrappee.clone(),
            max_burst: max_burst,
            cursor: 0,
        }));
        let stream = Stream(Link {
            uid: uid,
            body: body.clone(),
        });
        stream.register_wrappee_callback(&wrappee);
        stream
    }
} // impl Stream
