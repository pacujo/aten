use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::io::Result;
use std::time::{Instant, Duration};

use crate::{Disk, Link, Action, UID, Timer, Downgradable, error};
use crate::stream::{ByteStream, ByteStreamBody, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    ATEN_PACERSTREAM_DROP,
    ATEN_PACERSTREAM_UPPED_MISS,
    ATEN_PACERSTREAM_REGISTER_CALLBACK,
    ATEN_PACERSTREAM_UNREGISTER_CALLBACK,
    ATEN_PACERSTREAM_READ_TRIVIAL,
    ATEN_PACERSTREAM_READ,
    ATEN_PACERSTREAM_READ_DUMP,
    ATEN_PACERSTREAM_READ_TEXT,
    ATEN_PACERSTREAM_READ_FAIL);

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
    wrappee: ByteStream,
    byterate: f64,
    byteperiod: f64,
    quota: f64,
    min_burst: f64,
    max_burst: f64,
    prev_time: Instant,
    retry_timer: Option<Timer>,
    weak_self: Weak<RefCell<Self>>,
}

impl StreamBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        let disk =
            match self.base.get_weak_disk().upgrade() {
                Some(disk) => disk,
                None => { return Err(error::badf()); }
            };

        let now = disk.now();
        self.quota += (now - self.prev_time).as_secs_f64() * self.byterate;
        if self.quota > self.max_burst {
	    self.quota = self.max_burst;
        }
        self.prev_time = now;
        if self.quota < self.min_burst {
            let delay = (self.min_burst - self.quota) / self.byterate;
            TRACE!(ATEN_PACERSTREAM_READ_POSTPONE {
                STREAM: self, QUOTA: self.quota, DELAY: delay
            });
            let weak_self = self.weak_self.clone();
            self.retry_timer = Some(disk.schedule(
                now + Duration::from_secs_f64(delay),
                Action::new(move || {
                    if let Some(body) = weak_self.upgrade() {
                        body.borrow().retry();
                    };
                })));
            return Err(error::again());
        }
        self.retry_timer = None;
        let count = std::cmp::min(buf.len(), self.quota as usize);
        match self.wrappee.read(&mut buf[..count]) {
            Ok(n) => {
                let n_f64 = n as f64;
                assert!(n_f64 <= self.quota);
                self.quota -= n_f64;
                Ok(n)
            }
            Err(err) => {
                Err(err)
            }
        }
    }

    fn retry(&self) {
        TRACE!(ATEN_PACERSTREAM_RETRY { STREAM: self });
        self.base.invoke_callback();
    }
}

impl Stream {
    IMPL_STREAM!();

    pub fn new(disk: &Disk,
               wrappee: ByteStream,
               byterate: f64,
               min_burst: usize,
               max_burst: usize) -> Result<Stream> {
        if byterate <= 0.0 || min_burst < 1 || max_burst < min_burst {
            return Err(error::inval())
        }
        let uid = UID::new();
        TRACE!(ATEN_PACERSTREAM_CREATE {
            DISK: disk, STREAM: uid, WRAPPEE: wrappee,
            RATE: byterate, MIN_BURST: min_burst, MAX_BURST: max_burst,
        });
        let body = Rc::new_cyclic(
            |weak_self| RefCell::new(StreamBody {
                base: base::StreamBody::new(disk.downgrade(), uid),
                wrappee: wrappee.clone(),
                byterate: byterate,
                byteperiod: 1.0 / byterate,
                quota: 0.0,
                min_burst: min_burst as f64,
                max_burst: max_burst as f64,
                prev_time: disk.now(),
                retry_timer: None,
                weak_self: weak_self.clone(),
            }));
        let stream = Stream(Link {
            uid: uid,
            body: body,
        });
        stream.register_wrappee_callback(&wrappee);
        Ok(stream)
    }
} // impl Stream
