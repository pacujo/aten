use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Result, Error};

use crate::{Action, Disk, Link, UID, callback_to_string};
use crate::stream::{BaseStreamBody, ByteStreamBody};
use r3::TRACE;

DECLARE_STREAM!(DryStream, WeakDryStream, DryStreamBody, ATEN_DRYSTREAM_DROP);

#[derive(Debug)]
struct DryStreamBody {
    base: BaseStreamBody,
}

impl ByteStreamBody for DryStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Ok(n) = self.base.read(buf) {
            TRACE!(ATEN_DRYSTREAM_READ_TRIVIAL {
                STREAM: self, WANT: buf.len()
            });
            return Ok(n);
        }
        TRACE!(ATEN_DRYSTREAM_READ_FAIL {
            STREAM: self, WANT: buf.len(), ERR: "EAGAIN"
        });
        Err(Error::from_raw_os_error(libc::EAGAIN))
    }

    fn register(&mut self, callback: Option<Action>) {
        TRACE!(ATEN_DRYSTREAM_REGISTER {
            STREAM: self, CALLBACK: callback_to_string(&callback)
        });
        self.base.register(callback);
    }
} // impl ByteStreamBody for DryStreamBody

impl DryStream {
    IMPL_STREAM!(WeakDryStream);

    pub fn new(disk: &Disk) -> DryStream {
        let uid = UID::new();
        TRACE!(ATEN_DRYSTREAM_CREATE { DISK: disk, STREAM: uid });
        let body = DryStreamBody {
            base: BaseStreamBody::new(disk.downgrade(), uid)
        };
        DryStream(Link {
            uid: uid,
            body: Rc::new(RefCell::new(body)),
        })
    }
} // impl DryStream
