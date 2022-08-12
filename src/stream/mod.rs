#![allow(dead_code)]

use std::cell::RefCell;
use std::io::Result;
use std::rc::Rc;

use crate::{Action, Link, UID, WeakDisk, WeakLink};

pub struct ByteStream(Link<dyn ByteStreamBody>);

impl ByteStream {
    pub fn new(uid: UID, body: Rc<RefCell<dyn ByteStreamBody>>) -> ByteStream {
        ByteStream(Link {
            uid: uid,
            body: body,
        })
    }

    pub fn downgrade(&self) -> WeakByteStream {
        WeakByteStream(WeakLink {
            uid: self.0.uid,
            body: Rc::downgrade(&self.0.body),
        })
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        self.0.body.borrow_mut().read(buf)
    }

    pub fn close(&self) {
        self.0.body.borrow_mut().close();
    }

    pub fn register(&self, callback: Option<Action>) {
        self.0.body.borrow_mut().register(callback);
    }
} // impl ByteStream

impl std::clone::Clone for ByteStream {
    fn clone(&self) -> Self {
        ByteStream::new(self.0.uid, self.0.body.clone())
    }
} // impl std::clone::Clone for ByteStream

impl std::fmt::Debug for ByteStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ByteStream")
            .field("uid", &self.0.uid)
            .finish()
    }
} // impl Debug for ByteStream 

pub struct WeakByteStream(WeakLink<dyn ByteStreamBody>);

impl WeakByteStream {
    pub fn upgrade(&self) -> Option<ByteStream> {
        self.0.body.upgrade().map(|body|
            ByteStream(Link {
                uid: self.0.uid,
                body: body,
            }))
    }

    pub fn upped<F>(&self, f: F) where F: Fn(&ByteStream) {
        match self.upgrade() {
            Some(stream) => { f(&stream); }
            None => {
                //let _ = format!("{:?}", std::ptr::addr_of!(f));
                //FSTRACE(ATEN_UPPED_MISS, );
            }
        };
    }
} // impl WeakByteStream

pub trait ByteStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    fn close(&mut self);
    fn register(&mut self, callback: Option<super::Action>);
}

pub trait DebuggableByteStreamBody: ByteStreamBody + std::fmt::Debug {}

pub fn close_relaxed(weak_disk: &WeakDisk, stream: &ByteStream) {
    weak_disk.upped(|disk| {
        let weak_stream = stream.downgrade();
        disk.execute(Rc::new(move || {
            weak_stream.upped(|stream| {
                stream.close();
            });
        }));
    });
}

pub mod basestream;
pub type BaseStreamBody = basestream::BaseStreamBody;

pub mod emptystream;
pub type EmptyStream = emptystream::EmptyStream;

pub mod drystream;
pub type DryStream = drystream::DryStream;

pub mod zerostream;
pub type ZeroStream = zerostream::ZeroStream;

pub mod blobstream;
pub type BlobStream = blobstream::BlobStream;

pub mod queuestream;
pub type QueueStream = queuestream::QueueStream;
pub type WeakQueueStream = queuestream::WeakQueueStream;
