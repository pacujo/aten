use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Action, Disk, Link, UID};
use crate::stream::{ByteStream, BaseStreamBody, ByteStreamBody};
use crate::stream::{DebuggableByteStreamBody, callback_to_string};
use r3::TRACE;

#[derive(Debug)]
struct BlobStreamBody {
    base: BaseStreamBody,
    blob: Vec<u8>,
    cursor: usize,
}

impl std::fmt::Display for BlobStreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.base)
    }
} // impl std::fmt::Display for BlobStreamBody

impl ByteStreamBody for BlobStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Ok(n) = self.base.read(buf) {
            TRACE!(ATEN_BLOBSTREAM_READ_TRIVIAL {
                STREAM: self, WANT: buf.len()
            });
            return Ok(n);
        }
        let count = buf.len().min(self.blob.len() - self.cursor);
        for slot in &mut buf[..count].iter_mut() {
            *slot = self.blob[self.cursor];
            self.cursor += 1;
        }
        TRACE!(ATEN_BLOBSTREAM_READ {
            STREAM: self, WANT: buf.len(), GOT: count
        });
        TRACE!(ATEN_BLOBSTREAM_READ_DUMP {
            STREAM: self, DATA: r3::octets(&buf[..count])
        });
        Ok(count)
    }

    fn close(&mut self) {
        TRACE!(ATEN_BLOBSTREAM_CLOSE { STREAM: self });
        self.base.close();
    }

    fn register(&mut self, callback: Option<Action>) {
        TRACE!(ATEN_BLOBSTREAM_REGISTER {
            STREAM: self, CALLBACK: callback_to_string(&callback)
        });
        self.base.register(callback);
    }
} // impl ByteStreamBody for BlobStreamBody 

impl DebuggableByteStreamBody for BlobStreamBody {}

#[derive(Debug)]
pub struct BlobStream(Link<BlobStreamBody>);

impl BlobStream {
    pub fn new(disk: &Disk, blob: Vec<u8>) -> BlobStream {
        let uid = UID::new();
        TRACE!(ATEN_BLOBSTREAM_CREATE {
            DISK: disk, STREAM: uid, BLOB_LEN: blob.len()
        });
        TRACE!(ATEN_BLOBSTREAM_CREATE_DUMP {
            STREAM: uid, BLOB: r3::octets(&blob)
        });
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
