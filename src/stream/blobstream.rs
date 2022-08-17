use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Action, Disk, Link, UID, callback_to_string};
use crate::stream::{BaseStreamBody, ByteStreamBody};
use r3::TRACE;

DECLARE_STREAM!(BlobStream, WeakBlobStream, BlobStreamBody,
                ATEN_BLOBSTREAM_DROP);

#[derive(Debug)]
struct BlobStreamBody {
    base: BaseStreamBody,
    blob: Vec<u8>,
    cursor: usize,
}

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

    fn register(&mut self, callback: Option<Action>) {
        TRACE!(ATEN_BLOBSTREAM_REGISTER {
            STREAM: self, CALLBACK: callback_to_string(&callback)
        });
        self.base.register(callback);
    }
} // impl ByteStreamBody for BlobStreamBody 

impl BlobStream {
    IMPL_STREAM!(WeakBlobStream);

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
} // impl BlobStream
