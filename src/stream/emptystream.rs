use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Action, Disk, Link, UID};
use crate::stream::{ByteStream, BaseStreamBody, ByteStreamBody};
use crate::stream::{DebuggableByteStreamBody, callback_to_string};
use r3::TRACE;

#[derive(Debug)]
struct EmptyStreamBody(BaseStreamBody);

impl std::fmt::Display for EmptyStreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
} // impl std::fmt::Display for EmptyStreamBody

impl ByteStreamBody for EmptyStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        TRACE!(ATEN_EMPTYSTREAM_READ {
            STREAM: self, WANT: buf.len(), GOT: 0
        });
        Ok(0)
    }

    fn close(&mut self) {
        TRACE!(ATEN_EMPTYSTREAM_CLOSE { STREAM: self });
        self.0.close();
    }

    fn register(&mut self, callback: Option<Action>) {
        TRACE!(ATEN_EMPTYSTREAM_REGISTER {
            STREAM: self, CALLBACK: callback_to_string(&callback)
        });
        self.0.register(callback);
    }
} // impl ByteStreamBody for EmptyStreamBody

impl DebuggableByteStreamBody for EmptyStreamBody {}

#[derive(Debug)]
pub struct EmptyStream(Link<EmptyStreamBody>);

impl EmptyStream {
    pub fn new(disk: &Disk) -> EmptyStream {
        let uid = UID::new();
        TRACE!(ATEN_EMPTYSTREAM_CREATE { DISK: disk, STREAM: uid });
        let body = EmptyStreamBody(BaseStreamBody::new(
            disk.downgrade(), uid));
        EmptyStream(Link {
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
        self.0.body.borrow_mut().close();
    }

    fn register(&self, callback: Option<Action>) {
        self.0.body.borrow_mut().register(callback);
    }
} // impl EmptyStream

impl From<EmptyStream> for ByteStream {
    fn from(stream: EmptyStream) -> ByteStream {
        stream.as_byte_stream()
    }
} // impl From<EmptyStream> for ByteStream 
