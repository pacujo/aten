use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Action, Disk, Link, UID};
use crate::stream::{ByteStream, BaseStreamBody, ByteStreamBody};
use crate::stream::{DebuggableByteStreamBody, callback_to_string};
use r3::TRACE;

#[derive(Debug)]
struct ZeroStreamBody(BaseStreamBody);

impl std::fmt::Display for ZeroStreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
} // impl std::fmt::Display for ZeroStreamBody

impl ByteStreamBody for ZeroStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Ok(n) = self.0.read(buf) {
            TRACE!(ATEN_ZEROSTREAM_READ_TRIVIAL {
                STREAM: self, WANT: buf.len()
            });
            return Ok(n);
        }
        for slot in &mut buf.iter_mut() {
            *slot = 0;
        }
        TRACE!(ATEN_ZEROSTREAM_READ {
            STREAM: self, WANT: buf.len(), GOT: buf.len()
        });
        Ok(buf.len())
    }

    fn close(&mut self) {
        TRACE!(ATEN_ZEROSTREAM_CLOSE { STREAM: self });
        self.0.close();
    }

    fn register(&mut self, callback: Option<Action>) {
        TRACE!(ATEN_ZEROSTREAM_REGISTER {
            STREAM: self, CALLBACK: callback_to_string(&callback)
        });
        self.0.register(callback);
    }
} // impl ByteStreamBody for ZeroStreamBody 

impl DebuggableByteStreamBody for ZeroStreamBody {}

#[derive(Debug)]
pub struct ZeroStream(Link<ZeroStreamBody>);

impl ZeroStream {
    pub fn new(disk: &Disk) -> ZeroStream {
        let uid = UID::new();
        TRACE!(ATEN_ZEROSTREAM_CREATE { DISK: disk, STREAM: uid });
        let body = ZeroStreamBody(BaseStreamBody::new(
            disk.downgrade(), uid));
        ZeroStream(Link {
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
} // impl ZeroStream

impl From<ZeroStream> for ByteStream {
    fn from(stream: ZeroStream) -> ByteStream {
        stream.as_byte_stream()
    }
} // impl From<ZeroStream> for ByteStream 
