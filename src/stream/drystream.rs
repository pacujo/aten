use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Result, Error};

use crate::{Action, Disk, Link, UID};
use crate::stream::{ByteStream, BaseStreamBody, ByteStreamBody};
use crate::stream::{DebuggableByteStreamBody, callback_to_string};
use r3::TRACE;

#[derive(Debug)]
struct DryStreamBody(BaseStreamBody);

impl std::fmt::Display for DryStreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
} // impl std::fmt::Display for DryStreamBody

impl ByteStreamBody for DryStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Ok(n) = self.0.read(buf) {
            TRACE!(ATEN_DRYSTREAM_READ_TRIVIAL {
                STREAM: self, WANT: buf.len()
            });
            return Ok(n);
        }
        let err = Error::from_raw_os_error(libc::EAGAIN);
        TRACE!(ATEN_DRYSTREAM_READ_FAIL {
            STREAM: self, WANT: buf.len(), ERR: err
        });
        Err(err)
    }

    fn close(&mut self) {
        TRACE!(ATEN_DRYSTREAM_CLOSE { STREAM: self });
        self.0.close();
    }

    fn register(&mut self, callback: Option<Action>) {
        TRACE!(ATEN_DRYSTREAM_REGISTER {
            STREAM: self, CALLBACK: callback_to_string(&callback)
        });
        self.0.register(callback);
    }
} // impl ByteStreamBody for DryStreamBody

impl DebuggableByteStreamBody for DryStreamBody {}

#[derive(Debug)]
pub struct DryStream(Link<DryStreamBody>);

impl DryStream {
    pub fn new(disk: &Disk) -> DryStream {
        let uid = UID::new();
        TRACE!(ATEN_DRYSTREAM_CREATE { DISK: disk, STREAM: uid });
        let body = DryStreamBody(BaseStreamBody::new(
            disk.downgrade(), uid));
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
