use std::rc::Rc;
use std::cell::RefCell;
use std::collections::LinkedList;
use std::io::{Result, Error};

use crate::{Action, Disk, Link, UID, WeakLink, callback_to_string};
use crate::{again, is_again};
use crate::stream::{ByteStream, BaseStreamBody, ByteStreamBody};
use crate::stream::{DebuggableByteStreamBody, close_relaxed};
use r3::TRACE;

pub struct QueueStreamBody {
    base: BaseStreamBody,
    queue: LinkedList<ByteStream>,
    terminated: bool,
    pending_error: Option<Error>,
    notification: Option<Action>,
    notification_expected: bool,
}

crate::DISPLAY_BODY_UID!(QueueStreamBody);

impl ByteStreamBody for QueueStreamBody {
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
                    head.close();
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

    fn close(&mut self) {
        TRACE!(ATEN_QUEUESTREAM_CLOSE { STREAM: self });
        self.base.close();
    }

    fn register(&mut self, callback: Option<Action>) {
        TRACE!(ATEN_QUEUESTREAM_REGISTER {
            STREAM: self, CALLBACK: callback_to_string(&callback)
        });
        self.base.register(callback);
    }
} // impl ByteStreamBody for QueueStreamBody 

impl std::fmt::Debug for QueueStreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueueStreamBody")
            .field("base", &self.base)
            .field("queue", &self.queue)
            .field("terminated", &self.terminated)
            .field("pending_error", &self.pending_error)
            .field("notification", &self.notification.is_some())
            .field("notification_expected", &self.notification_expected)
            .finish()
    }
} // impl Debug for QueueStreamBody 

impl DebuggableByteStreamBody for QueueStreamBody {}

#[derive(Debug)]
pub struct QueueStream(Link<QueueStreamBody>);

crate::DISPLAY_LINK_UID!(QueueStream);

impl QueueStream {
    pub fn new(disk: &Disk) -> QueueStream {
        let uid = UID::new();
        TRACE!(ATEN_QUEUESTREAM_CREATE { DISK: disk, STREAM: uid });
        let body = Rc::new(RefCell::new(QueueStreamBody {
            base: BaseStreamBody::new(
                disk.downgrade(), uid),
            queue: LinkedList::new(),
            terminated: false,
            pending_error: None,
            notification: None,
            notification_expected: false,
        }));
        let stream = QueueStream(Link {
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

    fn get_callback(&self) -> Option<Action> {
        self.0.body.borrow().base.get_callback()
    }

    pub fn as_byte_stream(&self) -> ByteStream {
        ByteStream::new(self.0.uid, self.0.body.clone())
    }

    pub fn downgrade(&self) -> WeakQueueStream {
        WeakQueueStream(WeakLink {
            uid: self.0.uid,
            body: Rc::downgrade(&self.0.body),
        })
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
        if self.0.body.borrow().base.is_closed() {
            TRACE!(ATEN_QUEUESTREAM_ENQUEUE_POSTHUMOUSLY {
                STREAM: self, OTHER: other
            });
            close_relaxed(self.0.body.borrow().base.get_weak_disk(), other);
            return;
        }
        TRACE!(ATEN_QUEUESTREAM_ENQUEUE { STREAM: self, OTHER: other });
        self.0.body.borrow_mut().queue.push_back(other.clone());
        other.register(Some(self.make_notifier()));
        self.0.body.borrow().base.get_weak_disk().upped(
            |disk| { disk.execute(self.make_notifier()); }
        );        
    }
} // impl QueueStream

impl From<QueueStream> for ByteStream {
    fn from(stream: QueueStream) -> ByteStream {
        stream.as_byte_stream()
    }
} // impl From<QueueStream> for ByteStream 

pub struct WeakQueueStream(WeakLink<QueueStreamBody>);

impl WeakQueueStream {
    pub fn upgrade(&self) -> Option<QueueStream> {
        self.0.body.upgrade().map(|body|
            QueueStream(Link {
                uid: self.0.uid,
                body: body,
            }))
    }

    pub fn upped<F>(&self, f: F) where F: Fn(&QueueStream) {
        match self.upgrade() {
            Some(stream) => { f(&stream); }
            None => {
                TRACE!(ATEN_QUEUESTREAM_UPPED_MISS { STREAM: self });
            }
        };
    }
} // impl WeakQueueStream

crate::DISPLAY_LINK_UID!(WeakQueueStream);
