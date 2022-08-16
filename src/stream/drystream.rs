use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Result, Error};

use crate::{Action, Disk, Link, UID, callback_to_string};
use crate::stream::{ByteStream, BaseStreamBody, ByteStreamBody};
use crate::stream::DebuggableByteStreamBody;
use r3::TRACE;

#[derive(Debug)]
struct DryStreamBody {
    base: BaseStreamBody,
}

crate::DISPLAY_BODY_UID!(DryStreamBody);

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

    fn close(&mut self) {
        TRACE!(ATEN_DRYSTREAM_CLOSE { STREAM: self });
        self.base.close();
    }

    fn register(&mut self, callback: Option<Action>) {
        TRACE!(ATEN_DRYSTREAM_REGISTER {
            STREAM: self, CALLBACK: callback_to_string(&callback)
        });
        self.base.register(callback);
    }
} // impl ByteStreamBody for DryStreamBody

impl DebuggableByteStreamBody for DryStreamBody {}

#[derive(Debug)]
pub struct DryStream(Link<DryStreamBody>);

impl DryStream {
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

    pub fn as_byte_stream(&self) -> ByteStream {
        ByteStream::new(self.0.uid, self.0.body.clone())
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        self.0.body.borrow_mut().read(buf)
    }

    pub fn close(&self) {
        self.0.body.borrow_mut().close()
    }

    pub fn register(&self, callback: Option<Action>) {
        self.0.body.borrow_mut().register(callback)
    }
} // impl DryStream

impl From<DryStream> for ByteStream {
    fn from(stream: DryStream) -> ByteStream {
        stream.as_byte_stream()
    }
} // impl From<DryStream> for ByteStream 
