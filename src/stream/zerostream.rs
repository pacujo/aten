use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Action, Disk, Link, UID};
use crate::stream::{ByteStream, BaseStreamBody, ByteStreamBody};
use crate::stream::{DebuggableByteStreamBody};

#[derive(Debug)]
struct ZeroStreamBody(BaseStreamBody);

impl ByteStreamBody for ZeroStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Ok(n) = self.0.read(buf) {
            //FSTRACE(ASYNC_ZEROSTREAM_READ, count);
            return Ok(n);
        }
        for slot in &mut buf.iter_mut() {
            *slot = 0;
        }
        //FSTRACE(ASYNC_ZEROSTREAM_READ, count);
        Ok(buf.len())
    }

    fn close(&mut self) {
        //FSTRACE(ASYNC_ZEROSTREAM_CLOSE, count);
        self.0.close();
    }

    fn register(&mut self, callback: Option<Action>) {
        //FSTRACE(ASYNC_ZEROSTREAM_REGISTER, count);
        self.0.register(callback);
    }
} // impl ByteStreamBody for ZeroStreamBody 

impl DebuggableByteStreamBody for ZeroStreamBody {}

#[derive(Debug)]
pub struct ZeroStream(Link<ZeroStreamBody>);

impl ZeroStream {
    pub fn new(disk: &Disk) -> ZeroStream {
        //FSTRACE(ASYNC_ZEROSTREAM_CREATE, qstr->uid, qstr, async);
        let uid = UID::new();
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
