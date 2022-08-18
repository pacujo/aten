use std::rc::Rc;
use std::cell::RefCell;
use std::collections::LinkedList;
use std::io::{Result, Error};

use crate::{Action, Disk, Link, UID, callback_to_string};
use crate::{again, is_again};
use crate::stream::{ByteStream, ByteStreamBody, base};
use r3::TRACE;

DECLARE_STREAM!(ATEN_QUEUESTREAM_DROP, ATEN_QUEUESTREAM_UPPED_MISS);

pub struct StreamBody {
    base: base::StreamBody,
    queue: LinkedList<ByteStream>,
    terminated: bool,
    pending_error: Option<Error>,
    notification: Option<Action>,
    notification_expected: bool,
}

impl ByteStreamBody for StreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Ok(n) = self.base.read(buf) {
            TRACE!(ATEN_QUEUESTREAM_READ_TRIVIAL {
                STREAM: self, WANT: buf.len()
            });
            return Ok(n);
        }
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
        TRACE!(ATEN_QUEUESTREAM_READ {
            STREAM: self, WANT: buf.len(), GOT: cursor
        });
        if cursor > 0 {
            TRACE!(ATEN_QUEUESTREAM_READ_DUMP {
                STREAM: self, DATA: r3::octets(&buf[..cursor])
            });
            Ok(cursor)
        } else if self.terminated {
            Ok(0)
        } else {
            Err(again())
        }
    }

    fn register(&mut self, callback: Option<Action>) {
        TRACE!(ATEN_QUEUESTREAM_REGISTER {
            STREAM: self, CALLBACK: callback_to_string(&callback)
        });
        self.base.register(callback);
    }
} // impl ByteStreamBody for StreamBody 

impl std::fmt::Debug for StreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamBody")
            .field("base", &self.base)
            .field("queue", &self.queue)
            .field("terminated", &self.terminated)
            .field("pending_error", &self.pending_error)
            .field("notification", &self.notification.is_some())
            .field("notification_expected", &self.notification_expected)
            .finish()
    }
} // impl Debug for StreamBody 

impl Stream {
    IMPL_STREAM!();

    pub fn new(disk: &Disk) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_QUEUESTREAM_CREATE { DISK: disk, STREAM: uid });
        let body = Rc::new(RefCell::new(StreamBody {
            base: base::StreamBody::new(
                disk.downgrade(), uid),
            queue: LinkedList::new(),
            terminated: false,
            pending_error: None,
            notification: None,
            notification_expected: false,
        }));
        let stream = Stream(Link {
            uid: uid,
            body: body.clone(),
        });
        // "tie the knot"
        let weak_stream = stream.downgrade();
        let action = Rc::new(move || {
            weak_stream.upped(|stream| {
                if let Some(action) = stream.get_callback() {
                    (action)();
                }
            });
        });
        body.borrow_mut().notification = Some(action);
        stream
    }

    fn make_notifier(&self) -> Action {
        let weak_stream = self.downgrade();
        Rc::new(move || {
            weak_stream.upped(|stream| {
                let action =
                    if let Some(action) = &stream.0.body.borrow().notification {
                        action.clone()
                    } else {
                        unreachable!();
                    };
                (action)();
            });
        })
    }

    pub fn enqueue(&self, other: &ByteStream) {
        assert!(!self.0.body.borrow().terminated);
        TRACE!(ATEN_QUEUESTREAM_ENQUEUE { STREAM: self, OTHER: other });
        self.0.body.borrow_mut().queue.push_back(other.clone());
        other.register(Some(self.make_notifier()));
        self.0.body.borrow().base.get_weak_disk().upped(
            |disk| { disk.execute(self.make_notifier()); }
        );        
    }
} // impl Stream
