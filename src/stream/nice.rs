use std::rc::{Rc, Weak};
use std::cell::RefCell;
//use std::io::{Result, Error};
use std::io::Result;

use crate::{Disk, Link, UID, again};
use crate::stream::{ByteStream, ByteStreamBody, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    ATEN_NICESTREAM_DROP,
    ATEN_NICESTREAM_UPPED_MISS,
    ATEN_NICESTREAM_REGISTER_CALLBACK,
    ATEN_NICESTREAM_UNREGISTER_CALLBACK,
    ATEN_NICESTREAM_READ_TRIVIAL,
    ATEN_NICESTREAM_READ,
    ATEN_NICESTREAM_READ_DUMP,
    ATEN_NICESTREAM_READ_TEXT,
    ATEN_NICESTREAM_READ_FAIL);

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
    wrappee: ByteStream,
    max_burst: usize,
    cursor: usize,
    weak_self: Weak<RefCell<Self>>,
}

impl StreamBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.cursor >= self.max_burst {
            self.back_off();
            return Err(again());
        }
        match self.wrappee.read(buf) {
            Ok(n) => {
                self.cursor += n;
                Ok(n)
            }
            Err(err) => {
                self.cursor = 0;
                Err(err)
            }
        }
    }

    fn back_off(&mut self) {
        TRACE!(ATEN_NICESTREAM_BACK_OFF { STREAM: self });
        self.cursor = 0;
        self.base.get_weak_disk().upped(
            |disk| {
                let weak_self = self.weak_self.clone();
                disk.execute(Rc::new(move || {
                    weak_self.upgrade().map(|cell| {
                        cell.borrow().retry();
                    });
                }));
        });
    }

    fn retry(&self) {
        TRACE!(ATEN_NICESTREAM_RETRY { STREAM: self });
        if let Some(action) = self.base.get_callback() {
            (action)();
        }
    }
}

impl Stream {
    IMPL_STREAM!();

    pub fn new(disk: &Disk, wrappee: &ByteStream, max_burst: usize) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_NICESTREAM_CREATE {
            DISK: disk, STREAM: uid, WRAPPEE: wrappee, MAX_BURST: max_burst,
        });
        let body = Rc::new_cyclic(
            |weak_self| RefCell::new(StreamBody {
                base: base::StreamBody::new(disk.downgrade(), uid),
                wrappee: wrappee.clone(),
                max_burst: max_burst,
                cursor: 0,
                weak_self: weak_self.clone(),
            }));
        let stream = Stream(Link {
            uid: uid,
            body: body.clone(),
        });
        stream.register_wrappee_callback(wrappee);
        stream
    }
} // impl Stream
