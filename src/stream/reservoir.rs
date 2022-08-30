use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, Link, UID, Downgradable, error};
use crate::stream::{ByteStream, ByteStreamBody, base, queue, blob};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    ATEN_RESERVOIR_DROP,
    ATEN_RESERVOIR_UPPED_MISS,
    ATEN_RESERVOIR_REGISTER_CALLBACK,
    ATEN_RESERVOIR_UNREGISTER_CALLBACK,
    ATEN_RESERVOIR_READ_TRIVIAL,
    ATEN_RESERVOIR_READ,
    ATEN_RESERVOIR_READ_DUMP,
    ATEN_RESERVOIR_READ_TEXT,
    ATEN_RESERVOIR_READ_FAIL);

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
    wrappee: ByteStream,
    capacity: usize,
    amount: usize,
    eof_reached: bool,
    storage: queue::Stream,
}

impl StreamBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.eof_reached {
            return self.storage.read(buf);
        }
        loop {
            if self.amount > self.capacity {
                TRACE!(ATEN_RESERVOIR_OVERFLOW { STREAM: self });
                return Err(error::nospc());
            }
            let mut chunk = [0u8; 2000];
            match self.wrappee.read(&mut chunk) {
                Ok(0) => {
                    TRACE!(ATEN_RESERVOIR_FILLED { STREAM: self });
                    self.storage.terminate();
                    self.eof_reached = true;
                    return self.storage.read(buf);
                }
                Ok(n) => {
                    self.amount += n;
                    if let Some(disk) = self.base.get_weak_disk().upgrade() {
                        self.storage.enqueue(
                            blob::Stream::new(&disk, chunk[..n].to_vec())
                                .as_bytestream());
                    } else {
                        return Err(error::badf());
                    }
                }
                Err(err) => {
                    return Err(err);
                }
            }
        }
    }
}

impl Stream {
    IMPL_STREAM!();

    pub fn new(disk: &Disk, wrappee: ByteStream, capacity: usize) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_RESERVOIR_CREATE {
            DISK: disk, STREAM: uid, WRAPPEE: wrappee, CAPACITY: capacity,
        });
        let body = Rc::new(RefCell::new(StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
            wrappee: wrappee.clone(),
            capacity: capacity,
            amount: 0,
            eof_reached: false,
            storage: queue::Stream::new(&disk),
        }));
        let stream = Stream(Link {
            uid: uid,
            body: body.clone(),
        });
        stream.register_wrappee_callback(&wrappee);
        stream
    }

    pub fn amount(&self) -> usize {
        self.0.body.borrow().amount
    }
} // impl Stream
