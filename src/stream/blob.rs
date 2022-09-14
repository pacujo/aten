use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, Link, UID, Downgradable};
use crate::stream::{BasicStream, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    Stream, WeakStream, StreamBody,
    ATEN_BLOBSTREAM_DROP,
    ATEN_BLOBSTREAM_UPPED_MISS,
    ATEN_BLOBSTREAM_REGISTER_CALLBACK,
    ATEN_BLOBSTREAM_UNREGISTER_CALLBACK,
    ATEN_BLOBSTREAM_READ_TRIVIAL,
    ATEN_BLOBSTREAM_READ,
    ATEN_BLOBSTREAM_READ_DUMP,
    ATEN_BLOBSTREAM_READ_FAIL);

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
    blob: Vec<u8>,
    cursor: usize,
}

impl StreamBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        let count = buf.len().min(self.blob.len() - self.cursor);
        for slot in &mut buf[..count].iter_mut() {
            *slot = self.blob[self.cursor];
            self.cursor += 1;
        }
        Ok(count)
    }
}

impl Stream {
    pub fn new(disk: &Disk, blob: Vec<u8>) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_BLOBSTREAM_CREATE {
            DISK: disk, STREAM: uid, BLOB_LEN: blob.len()
        });
        TRACE!(ATEN_BLOBSTREAM_CREATE_DUMP {
            STREAM: uid, DATA: r3::octets(&blob)
        });
        let body = StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
            blob: blob,
            cursor: 0,
        };
        Stream(Link {
            uid: uid,
            body: Rc::new(RefCell::new(body)),
        })
    }
} // impl Stream
