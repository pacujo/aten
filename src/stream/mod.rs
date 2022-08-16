#![allow(dead_code)]

use std::cell::RefCell;
use std::io::Result;
use std::rc::Rc;

use crate::{Action, Link, UID, WeakDisk, WeakLink};
use r3::TRACE;

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

crate::DISPLAY_LINK_UID!(ByteStream);

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

crate::DISPLAY_LINK_UID!(WeakByteStream);

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
                TRACE!(ATEN_BYTESTREAM_UPPED_MISS { STREAM: self });
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

#[macro_export]
macro_rules! DISPLAY_BODY_UID {
    ($typename:ident) => {
        impl std::fmt::Display for $typename {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.base)
            }
        } // impl std::fmt::Display for DryStreamBody
    }
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
