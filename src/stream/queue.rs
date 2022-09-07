use std::rc::Rc;
use std::cell::RefCell;
use std::collections::LinkedList;
use std::io::{Result, Error, Write};

use crate::{Disk, Link, UID, Downgradable, error};
use crate::stream::{ByteStream, ByteStreamBody, base, blob};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    ATEN_QUEUESTREAM_DROP,
    ATEN_QUEUESTREAM_UPPED_MISS,
    ATEN_QUEUESTREAM_REGISTER_CALLBACK,
    ATEN_QUEUESTREAM_UNREGISTER_CALLBACK,
    ATEN_QUEUESTREAM_READ_TRIVIAL,
    ATEN_QUEUESTREAM_READ,
    ATEN_QUEUESTREAM_READ_DUMP,
    ATEN_QUEUESTREAM_READ_FAIL);

pub struct StreamBody {
    base: base::StreamBody,
    queue: LinkedList<ByteStream>,
    terminated: bool,
    supplier: Option<Rc<RefCell<dyn Supplier>>>,
    exhausted: bool,
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
                        if error::is_again(&err) {
                            self.notification_expected = true;
                        }
                        return Err(err);
                    }
                    if !error::is_again(&err) {
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
            self.exhausted = true;
            Ok(0)
        } else {
            Err(error::again())
        }
    }
}

impl std::fmt::Debug for StreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("queue::Stream")
         .field("base", &self.base)
         .field("queue", &self.queue)
         .field("terminated", &self.terminated)
         .field("supplier", &self.supplier.is_some())
         .field("exhausted", &self.exhausted)
         .field("pending_error", &self.pending_error)
         .field("notification_expected", &self.notification_expected)
         .finish()
    }
} // impl std::fmt::Debug for StreamBody

pub trait Supplier {}

impl Stream {
    IMPL_STREAM!();

    pub fn new(disk: &Disk, supplier: Option<Rc<RefCell<dyn Supplier>>>)
               -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_QUEUESTREAM_CREATE { DISK: disk, STREAM: uid });
        let body = Rc::new(RefCell::new(StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
            queue: LinkedList::new(),
            supplier: supplier,
            terminated: false,
            exhausted: false,
            pending_error: None,
            notification_expected: false,
        }));
        Stream(Link {
            uid: uid,
            body: body.clone(),
        })
    }

    pub fn enqueue(&self, wrappee: ByteStream) {
        assert!(!self.0.body.borrow().terminated);
        TRACE!(ATEN_QUEUESTREAM_ENQUEUE { STREAM: self, WRAPPEE: wrappee });
        self.0.body.borrow_mut().queue.push_back(wrappee.clone());
        self.register_wrappee_callback(&wrappee);
    }

    pub fn push(&self, wrappee: ByteStream) {
        assert!(!self.0.body.borrow().exhausted);
        TRACE!(ATEN_QUEUESTREAM_PUSH { STREAM: self, WRAPPEE: wrappee });
        self.0.body.borrow_mut().queue.push_front(wrappee.clone());
        self.register_wrappee_callback(&wrappee);
    }

    pub fn terminate(&self) {
        assert!(!self.0.body.borrow().terminated);
        self.0.body.borrow_mut().terminated = true;
        self.0.body.borrow_mut().supplier = None;
        self.invoke_callback();
    }
} // impl Stream

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let weak_disk = self.0.body.borrow().base.get_weak_disk().clone();
        match weak_disk.upgrade() {
            Some(disk) => {
                let count = buf.len();
                self.enqueue(
                    blob::Stream::new(&disk, buf.to_vec()).as_bytestream());
                Ok(count)
            }
            None => {
                Err(error::badf())
            }
        }
    }

    fn flush(&mut self) -> Result<()> {
        Err(error::inval())
    }
}
