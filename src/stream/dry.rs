use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Result, Error};

use crate::{Action, Disk, Link, UID, callback_to_string};
use crate::stream::{ByteStreamBody, base};
use r3::TRACE;

DECLARE_STREAM!(ATEN_DRYSTREAM_DROP, ATEN_DRYSTREAM_UPPED_MISS);

#[derive(Debug)]
struct StreamBody {
    base: base::StreamBody,
}

impl StreamBody {
    fn read_nontrivial(&mut self, _buf: &mut [u8]) -> Result<usize> {
        Err(Error::from_raw_os_error(libc::EAGAIN))
    }
}

impl ByteStreamBody for StreamBody {
    IMPL_STREAM_BODY!(ATEN_DRYSTREAM_REGISTER);

    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Ok(n) = self.base.read(buf) {
            TRACE!(ATEN_DRYSTREAM_READ_TRIVIAL {
                STREAM: self, WANT: buf.len()
            });
            return Ok(n);
        }
        match self.read_nontrivial(buf) {
            Ok(count) => {
                TRACE!(ATEN_DRYSTREAM_READ {
                    STREAM: self, WANT: buf.len(), GOT: count
                });
                TRACE!(ATEN_DRYSTREAM_READ_DUMP {
                    STREAM: self, DATA: r3::octets(&buf[..count])
                });
                Ok(count)
            }
            Err(err) => {
                TRACE!(ATEN_DRYSTREAM_READ_FAIL {
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
