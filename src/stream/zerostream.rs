use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Action, Disk, Link, UID, callback_to_string};
use crate::stream::{ByteStream, BaseStreamBody, ByteStreamBody};
use crate::stream::DebuggableByteStreamBody;
use r3::TRACE;

#[derive(Debug)]
struct ZeroStreamBody {
    base: BaseStreamBody,
}

crate::DISPLAY_BODY_UID!(ZeroStreamBody);

impl ByteStreamBody for ZeroStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Ok(n) = self.base.read(buf) {
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
        self.base.close();
    }

    fn register(&mut self, callback: Option<Action>) {
        TRACE!(ATEN_ZEROSTREAM_REGISTER {
            STREAM: self, CALLBACK: callback_to_string(&callback)
        });
        self.base.register(callback);
    }
} // impl ByteStreamBody for ZeroStreamBody 

impl DebuggableByteStreamBody for ZeroStreamBody {}

#[derive(Debug)]
pub struct ZeroStream(Link<ZeroStreamBody>);

impl ZeroStream {
    pub fn new(disk: &Disk) -> ZeroStream {
        let uid = UID::new();
        TRACE!(ATEN_ZEROSTREAM_CREATE { DISK: disk, STREAM: uid });
        let body = ZeroStreamBody {
            base: BaseStreamBody::new(disk.downgrade(), uid)
        };
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
