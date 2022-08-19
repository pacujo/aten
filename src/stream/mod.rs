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
    ($drop:ident,
     $up_miss:ident,
     $register:ident,
     $trivial:ident,
     $read:ident,
     $read_dump:ident,
     $read_fail:ident) => {
        impl crate::stream::ByteStreamBody for StreamBody {
            fn register(&mut self, callback: Option<crate::Action>) {
                TRACE!($register {
                    STREAM: self, CALLBACK: crate::callback_to_string(&callback)
                });
                self.base.register(callback);
            }

            fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
                if let Ok(_) = self.base.read(buf) {
                    TRACE!($trivial { STREAM: self, WANT: buf.len() });
                    return Ok(0);
                }
                match self.read_nontrivial(buf) {
                    Ok(count) => {
                        TRACE!($read {
                            STREAM: self, WANT: buf.len(), GOT: count
                        });
                        TRACE!($read_dump {
                            STREAM: self, DATA: r3::octets(&buf[..count])
                        });
                        Ok(count)
                    }
                    Err(err) => {
                        TRACE!($read_fail {
                            STREAM: self, WANT: buf.len(), ERR: r3::errsym(&err)
                        });
                        Err(err)
                    }
                }
            }
        }

        impl std::fmt::Display for StreamBody {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.base)
            }
        }

        impl Drop for StreamBody {
            fn drop(&mut self) {
                TRACE!($drop { STREAM: self });
            }
        }

        impl crate::stream::DebuggableByteStreamBody for StreamBody {}

        #[derive(Debug)]
        pub struct Stream(crate::Link<StreamBody>);

        impl From<Stream> for crate::stream::ByteStream {
            fn from(stream: Stream) -> crate::stream::ByteStream {
                stream.as_byte_stream()
            }
        }

        #[derive(Debug)]
        pub struct WeakStream(crate::WeakLink<StreamBody>);

        impl WeakStream {
            pub fn upgrade(&self) -> Option<Stream> {
                self.0.body.upgrade().map(|body|
                                          Stream(Link {
                                              uid: self.0.uid,
                                              body: body,
                                          }))
            }

            pub fn upped<F>(&self, f: F) where F: Fn(&Stream) {
                match self.upgrade() {
                    Some(stream) => { f(&stream); }
                    None => {
                        TRACE!($up_miss { STREAM: self });
                    }
                };
            }
        }

        crate::DISPLAY_LINK_UID!(Stream);
        crate::DISPLAY_LINK_UID!(WeakStream);
    }
}

#[macro_export]
macro_rules! IMPL_STREAM {
    () => {
        fn get_callback(&self) -> Option<crate::Action> {
            self.0.body.borrow().base.get_callback()
        }

        pub fn as_byte_stream(&self) -> crate::stream::ByteStream {
            crate::stream::ByteStream::new(self.0.uid, self.0.body.clone())
        }

        pub fn downgrade(&self) -> WeakStream {
            WeakStream(crate::WeakLink {
                uid: self.0.uid,
                body: std::rc::Rc::downgrade(&self.0.body),
            })
        }

        pub fn read(&self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.body.borrow_mut().read(buf)
        }

        pub fn register(&self, callback: Option<crate::Action>) {
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
