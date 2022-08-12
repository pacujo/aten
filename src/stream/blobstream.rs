use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Action, Disk, Link, UID};
use crate::stream::{ByteStream, BaseStreamBody, ByteStreamBody};
use crate::stream::{DebuggableByteStreamBody};

#[derive(Debug)]
struct BlobStreamBody {
    base: BaseStreamBody,
    blob: Vec<u8>,
    cursor: usize,
}

impl ByteStreamBody for BlobStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Ok(n) = self.base.read(buf) {
            //FSTRACE(ASYNC_BLOBSTREAM_READ, count);
            return Ok(n);
        }
        let wanted = buf.len().min(self.blob.len() - self.cursor);
        for slot in &mut buf[..wanted].iter_mut() {
            *slot = self.blob[self.cursor];
            self.cursor += 1;
        }
        //FSTRACE(ASYNC_BLOBSTREAM_READ, count);
        Ok(wanted)
    }

    fn close(&mut self) {
        //FSTRACE(ASYNC_BLOBSTREAM_CLOSE, count);
        self.base.close();
    }

    fn register(&mut self, callback: Option<Action>) {
        //FSTRACE(ASYNC_BLOBSTREAM_REGISTER, count);
        self.base.register(callback);
    }
} // impl ByteStreamBody for BlobStreamBody 

impl DebuggableByteStreamBody for BlobStreamBody {}

#[derive(Debug)]
pub struct BlobStream(Link<BlobStreamBody>);

impl BlobStream {
    pub fn new(disk: &Disk, blob: Vec<u8>) -> BlobStream {
        //FSTRACE(ASYNC_BLOBSTREAM_CREATE, qstr->uid, qstr, async);
        let uid = UID::new();
        let body = BlobStreamBody {
            base: BaseStreamBody::new(disk.downgrade(), uid),
            blob: blob,
            cursor: 0,
        };
        BlobStream(Link {
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
} // impl BlobStream

impl From<BlobStream> for ByteStream {
    fn from(stream: BlobStream) -> ByteStream {
        stream.as_byte_stream()
    }
} // impl From<BlobStream> for ByteStream 
