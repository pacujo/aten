use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Action, Disk, Link, UID};
use crate::stream::{ByteStream, BaseStreamBody, ByteStreamBody};
use crate::stream::{DebuggableByteStreamBody};

#[derive(Debug)]
struct EmptyStreamBody(BaseStreamBody);

impl ByteStreamBody for EmptyStreamBody {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
        //FSTRACE(ASYNC_EMPTYSTREAM_READ, count);
        Ok(0)
    }

    fn close(&mut self) {
        //FSTRACE(ASYNC_EMPTYSTREAM_CLOSE, count);
        self.0.close();
    }

    fn register(&mut self, callback: Option<Action>) {
        //FSTRACE(ASYNC_EMPTYSTREAM_REGISTER, count);
        self.0.register(callback);
    }
} // impl ByteStreamBody for EmptyStreamBody

impl DebuggableByteStreamBody for EmptyStreamBody {}

#[derive(Debug)]
pub struct EmptyStream(Link<EmptyStreamBody>);

impl EmptyStream {
    pub fn new(disk: &Disk) -> EmptyStream {
        //FSTRACE(ASYNC_EMPTYSTREAM_CREATE, qstr->uid, qstr, async);
        let uid = UID::new();
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
