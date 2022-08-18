use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Action, Disk, Link, UID, callback_to_string};
use crate::stream::{ByteStreamBody, base};
use r3::TRACE;

DECLARE_STREAM!(ATEN_EMPTYSTREAM_DROP, ATEN_EMPTYSTREAM_UPPED_MISS);

#[derive(Debug)]
struct StreamBody {
    base: base::StreamBody,
}

impl StreamBody {
    fn read_nontrivial(&mut self, _buf: &mut [u8]) -> Result<usize> {
        Ok(0)
    }
}

impl ByteStreamBody for StreamBody {
    IMPL_STREAM_BODY!(ATEN_EMPTYSTREAM_REGISTER);

    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        TRACE!(ATEN_EMPTYSTREAM_READ {
            STREAM: self, WANT: buf.len(), GOT: 0
        });
        match self.read_nontrivial(buf) {
            Ok(count) => {
                TRACE!(ATEN_EMPTYSTREAM_READ {
                    STREAM: self, WANT: buf.len(), GOT: count
                });
                TRACE!(ATEN_EMPTYSTREAM_READ_DUMP {
                    STREAM: self, DATA: r3::octets(&buf[..count])
                });
                Ok(count)
            }
            Err(err) => {
                TRACE!(ATEN_EMPTYSTREAM_READ_FAIL {
                    STREAM: self, WANT: buf.len(), ERR: r3::errsym(&err)
                });
                Err(err)
            }
        }
    }
} // impl ByteStreamBody for StreamBody

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
