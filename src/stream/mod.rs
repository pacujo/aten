#![allow(dead_code)]

use std::cell::RefCell;
use std::io::Result;
use std::rc::Rc;

use crate::{Action, Link, UID, WeakLink};
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
    fn register(&mut self, callback: Option<super::Action>);
}

pub trait DebuggableByteStreamBody: ByteStreamBody + std::fmt::Debug {}

#[macro_export]
macro_rules! DECLARE_STREAM {
    ($stream:ident, $weak:ident, $body:ident, $drop:ident) => {
        impl std::fmt::Display for $body {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.base)
            }
        }

        impl Drop for $body {
            fn drop(&mut self) {
                TRACE!($drop { STREAM: self });
            }
        }

        impl crate::stream::DebuggableByteStreamBody for $body {}

        #[derive(Debug)]
        pub struct $stream(crate::Link<$body>);

        impl From<$stream> for crate::stream::ByteStream {
            fn from(stream: $stream) -> crate::stream::ByteStream {
                stream.as_byte_stream()
            }
        }

        #[derive(Debug)]
        pub struct $weak(crate::WeakLink<$body>);

        impl $weak {
            pub fn upgrade(&self) -> Option<$stream> {
                self.0.body.upgrade().map(|body|
                                          $stream(Link {
                                              uid: self.0.uid,
                                              body: body,
                                          }))
            }

            pub fn upped<F>(&self, f: F) where F: Fn(&$stream) {
                match self.upgrade() {
                    Some(stream) => { f(&stream); }
                    None => {
                        TRACE!(ATEN_QUEUESTREAM_UPPED_MISS { STREAM: self });
                    }
                };
            }
        }

        crate::DISPLAY_LINK_UID!($stream);
        crate::DISPLAY_LINK_UID!($weak);
    }
}

#[macro_export]
macro_rules! IMPL_STREAM {
    ($weak:ident) => {
        fn get_callback(&self) -> Option<Action> {
            self.0.body.borrow().base.get_callback()
        }

        pub fn as_byte_stream(&self) -> crate::stream::ByteStream {
            crate::stream::ByteStream::new(self.0.uid, self.0.body.clone())
        }

        pub fn downgrade(&self) -> $weak {
            $weak(crate::WeakLink {
                uid: self.0.uid,
                body: std::rc::Rc::downgrade(&self.0.body),
            })
        }

        pub fn read(&self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.body.borrow_mut().read(buf)
        }

        pub fn register(&self, callback: Option<Action>) {
            self.0.body.borrow_mut().register(callback)
        }
    }
}

pub mod base;
pub mod blob;
pub mod dry;
pub mod empty;
pub mod queue;
pub mod zero;
