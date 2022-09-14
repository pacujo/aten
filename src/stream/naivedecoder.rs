use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, Link, UID, Downgradable, Upgradable, error};
use crate::stream::{ByteStream, ByteStreamBody, BasicStream};
use crate::stream::{base, queue, blob};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    Stream, WeakStream, StreamBody,
    ATEN_NAIVEDECODER_DROP,
    ATEN_NAIVEDECODER_UPPED_MISS,
    ATEN_NAIVEDECODER_REGISTER_CALLBACK,
    ATEN_NAIVEDECODER_UNREGISTER_CALLBACK,
    ATEN_NAIVEDECODER_READ_TRIVIAL,
    ATEN_NAIVEDECODER_READ,
    ATEN_NAIVEDECODER_READ_DUMP,
    ATEN_NAIVEDECODER_READ_FAIL);

#[derive(Debug)]
enum State {
    Reading,
    Escaped,
    Terminated(ByteStream),
    Errored,
}

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
    wrappee: ByteStream,
    state: State,
    terminator: u8,
    escape: Option<u8>,
}

impl StreamBody {
    fn decode(&mut self, buf: &mut [u8], count: usize) -> Result<usize> {
        assert!(count >= 1);
        let mut ri = 0;
        let mut wi = 0;
        if matches!(self.state, State::Escaped) {
            buf[wi] = buf[ri];
            wi += 1;
            ri += 1;
        }
        loop {
            if ri >= count {
                self.state = State::Reading;
                return if wi == 0 { self.read(buf) } else { Ok(wi) };
            }
            if buf[ri] == self.terminator {
                ri += 1;
                if ri == buf.len() {
                    self.state = State::Terminated(self.wrappee.clone());
                    return Ok(wi);
                }
                if let Some(disk) = self.base.get_weak_disk().upgrade() {
                    let q = queue::Stream::new(&disk, None);
                    q.enqueue(
                        blob::Stream::new(&disk, buf[ri..].to_vec())
                            .as_bytestream());
                    q.enqueue(self.wrappee.clone());
                    q.terminate();
                    self.state = State::Terminated(q.as_bytestream());
                    return Ok(wi);
                }
                return Err(error::badf());
            }
            if let Some(escape) = self.escape {
	        if buf[ri] == escape {
                    ri += 1;
                    if ri >= count {
                        self.state = State::Escaped;
                        return if wi == 0 { self.read(buf) } else { Ok(wi) };
                    }
                }
            }
            buf[wi] = buf[ri];
            wi += 1;
            ri += 1;
        }
    }

    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        match self.state {
            State::Reading | State::Escaped => {
                match self.wrappee.read(buf) {
                    Ok(0) => {
                        self.state = State::Errored;
                        Err(error::proto())
                    }
                    Ok(count) => {
                        self.decode(buf, count)
                    }
                    Err(err) => {
                        Err(err)
                    }
                }
            }
            State::Terminated(_) => {
                Ok(0)
            }
            State::Errored => {
                Err(error::proto())
            }
        }
    }
}

impl Stream {
    pub fn new(disk: &Disk,
               wrappee: ByteStream,
               terminator: u8,
               escape: Option<u8>) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_NAIVEDECODER_CREATE {
            DISK: disk, STREAM: uid, WRAPPEE: wrappee,
            TERMINATOR: terminator, ESCAPE: r3::option(&escape),
        });
        let body = Rc::new(RefCell::new(StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
            wrappee: wrappee.clone(),
            state: State::Reading,
            terminator: terminator,
            escape: escape,
        }));
        let stream = Stream(Link {
            uid: uid,
            body: body.clone(),
        });
        stream.register_wrappee_callback(&wrappee);
        stream
    }

    pub fn remainder(&self) -> Option<ByteStream> {
        let body = self.0.body.borrow();
        if let State::Terminated(stream) = &body.state {
            Some(stream.clone())
        } else {
            None
        }
    }
} // impl Stream
