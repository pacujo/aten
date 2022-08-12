use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Result, Error};

use crate::{Action, Disk, Link, UID};
use crate::stream::{ByteStream, BaseStreamBody, ByteStreamBody};
use crate::stream::{DebuggableByteStreamBody};

#[derive(Debug)]
struct DryStreamBody(BaseStreamBody);

impl ByteStreamBody for DryStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Ok(n) = self.0.read(buf) {
            //FSTRACE(ASYNC_DRYSTREAM_READ, count);
            return Ok(n);
        }
        //FSTRACE(ASYNC_DRYSTREAM_READ, count);
        Err(Error::from_raw_os_error(libc::EAGAIN))
    }

    fn close(&mut self) {
        //FSTRACE(ASYNC_DRYSTREAM_CLOSE, count);
        self.0.close();
    }

    fn register(&mut self, callback: Option<Action>) {
        //FSTRACE(ASYNC_DRYSTREAM_REGISTER, count);
        self.0.register(callback);
    }
} // impl ByteStreamBody for DryStreamBody

impl DebuggableByteStreamBody for DryStreamBody {}

#[derive(Debug)]
pub struct DryStream(Link<DryStreamBody>);

impl DryStream {
    pub fn new(disk: &Disk) -> DryStream {
        //FSTRACE(ASYNC_DRYSTREAM_CREATE, qstr->uid, qstr, async);
        let uid = UID::new();
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
