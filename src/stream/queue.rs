use std::rc::Rc;
use std::cell::RefCell;
use std::collections::LinkedList;
use std::io::{Result, Error};

use crate::{Disk, Link, UID};
use crate::{again, is_again};
use crate::stream::{ByteStream, ByteStreamBody, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    ATEN_QUEUESTREAM_DROP,
    ATEN_QUEUESTREAM_UPPED_MISS,
    ATEN_QUEUESTREAM_REGISTER_CALLBACK,
    ATEN_QUEUESTREAM_UNREGISTER_CALLBACK,
    ATEN_QUEUESTREAM_READ_TRIVIAL,
    ATEN_QUEUESTREAM_READ,
    ATEN_QUEUESTREAM_READ_DUMP,
    ATEN_QUEUESTREAM_READ_TEXT,
    ATEN_QUEUESTREAM_READ_FAIL);

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
    queue: LinkedList<ByteStream>,
    terminated: bool,
    pending_error: Option<Error>,
    notification_expected: bool,
}

impl StreamBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Some(err) = self.pending_error.take() {
            return Err(err);
        }
        let mut cursor = 0;
        while let Some(head) = self.queue.front_mut() {
            if cursor >= buf.len() {
                break;
            }
            match head.read(&mut buf[cursor..]) {
                Err(err) => {
                    if cursor == 0 {
                        if is_again(&err) {
                            self.notification_expected = true;
                        }
                        return Err(err);
                    }
                    if !is_again(&err) {
                        self.pending_error = Some(err);
                    }
                    break;
                }
                Ok(0) => {
                    self.queue.pop_front();
                }
                Ok(count) => {
                    cursor += count;
                }
            }
        }
        if cursor > 0 {
            Ok(cursor)
        } else if self.terminated {
            Ok(0)
        } else {
            Err(again())
        }
    }
}

impl Stream {
    IMPL_STREAM!();

    pub fn new(disk: &Disk) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_QUEUESTREAM_CREATE { DISK: disk, STREAM: uid });
        let body = Rc::new(RefCell::new(StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
            queue: LinkedList::new(),
            terminated: false,
            pending_error: None,
            notification_expected: false,
        }));
        Stream(Link {
            uid: uid,
            body: body.clone(),
        })
    }

    pub fn enqueue(&self, wrappee: &ByteStream) {
        assert!(!self.0.body.borrow().terminated);
        TRACE!(ATEN_QUEUESTREAM_ENQUEUE { STREAM: self, WRAPPEE: wrappee });
        self.0.body.borrow_mut().queue.push_back(wrappee.clone());
        self.register_wrappee_callback(wrappee);
    }
} // impl Stream
