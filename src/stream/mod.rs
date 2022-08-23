#![allow(dead_code)]

use std::cell::RefCell;
use std::io::Result;
use std::rc::Rc;

use crate::{Action, Link, UID, WeakLink};
use r3::{TRACE, Traceable};

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

    pub fn register_callback(&self, callback: Action) {
        self.0.body.borrow_mut().register_callback(callback);
    }

    pub fn unregister_callback(&self) {
        self.0.body.borrow_mut().unregister_callback();
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

    pub fn upped<F, R>(&self, f: F) -> Option<R> where F: Fn(&ByteStream) -> R {
        match self.upgrade() {
            Some(stream) => Some(f(&stream)),
            None => {
                TRACE!(ATEN_BYTESTREAM_UPPED_MISS { STREAM: self });
                None
            }
        }
    }
} // impl WeakByteStream

pub trait ByteStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    fn register_callback(&mut self, callback: Action);
    fn unregister_callback(&mut self);
}

pub trait DebuggableByteStreamBody: ByteStreamBody + std::fmt::Debug {}

#[macro_export]
macro_rules! DECLARE_STREAM {
    ($ATEN_STREAM_DROP:ident,
     $ATEN_STREAM_UPPED_MISS:ident,
     $ATEN_STREAM_REGISTER_CALLBACK:ident,
     $ATEN_STREAM_UNREGISTER_CALLBACK:ident,
     $ATEN_STREAM_READ_TRIVIAL:ident,
     $ATEN_STREAM_READ:ident,
     $ATEN_STREAM_READ_DUMP:ident,
     $ATEN_STREAM_READ_TEXT:ident,
     $ATEN_STREAM_READ_FAIL:ident) => {
        impl crate::stream::ByteStreamBody for StreamBody {
            fn register_callback(&mut self, callback: crate::Action) {
                TRACE!($ATEN_STREAM_REGISTER_CALLBACK {
                    STREAM: self, CALLBACK: crate::action_to_string(&callback)
                });
                self.base.register_callback(callback);
            }

            fn unregister_callback(&mut self) {
                TRACE!($ATEN_STREAM_UNREGISTER_CALLBACK { STREAM: self });
                self.base.unregister_callback();
            }

            fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
                if let Ok(_) = self.base.read(buf) {
                    TRACE!($ATEN_STREAM_READ_TRIVIAL {
                        STREAM: self, WANT: buf.len()
                    });
                    return Ok(0);
                }
                match self.read_nontrivial(buf) {
                    Ok(count) => {
                        TRACE!($ATEN_STREAM_READ {
                            STREAM: self, WANT: buf.len(), GOT: count
                        });
                        TRACE!($ATEN_STREAM_READ_DUMP {
                            STREAM: self, DATA: r3::octets(&buf[..count])
                        });
                        TRACE!($ATEN_STREAM_READ_TEXT {
                            STREAM: self, TEXT: r3::text(&buf[..count])
                        });
                        Ok(count)
                    }
                    Err(err) => {
                        TRACE!($ATEN_STREAM_READ_FAIL {
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
                TRACE!($ATEN_STREAM_DROP { STREAM: self });
            }
        }

        impl crate::stream::DebuggableByteStreamBody for StreamBody {}

        #[derive(Debug)]
        pub struct Stream(crate::Link<StreamBody>);

        impl From<Stream> for crate::stream::ByteStream {
            fn from(stream: Stream) -> crate::stream::ByteStream {
                stream.as_bytestream()
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

            pub fn upped<F, R>(&self, f: F) -> Option<R>
            where F: Fn(&Stream) -> R {
                match self.upgrade() {
                    Some(stream) => Some(f(&stream)),
                    None => {
                        TRACE!($ATEN_STREAM_UPPED_MISS { STREAM: self });
                        None
                    }
                }
            }
        }

        crate::DISPLAY_LINK_UID!(Stream);
        crate::DISPLAY_LINK_UID!(WeakStream);
    }
}

#[macro_export]
macro_rules! IMPL_STREAM {
    () => {
        pub fn get_callback(&self) -> Option<crate::Action> {
            self.0.body.borrow().base.get_callback()
        }

        pub fn as_bytestream(&self) -> crate::stream::ByteStream {
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

        pub fn register_callback(&self, callback: crate::Action) {
            self.0.body.borrow_mut().register_callback(callback);
        }

        pub fn unregister_callback(&self) {
            self.0.body.borrow_mut().unregister_callback();
        }

        pub fn register_wrappee_callback(
            &self, wrappee: &crate::stream::ByteStream) {
            let weak_stream = self.downgrade();
            wrappee.register_callback(Rc::new(move || {
                weak_stream.upped(|stream| {
                    if let Some(action) = stream.get_callback() {
                        (action)();
                    }
                });
            }));
        }
    }
}

pub mod base;
pub mod blob;
pub mod dry;
pub mod empty;
pub mod file;
pub mod naivedecoder;
pub mod naiveencoder;
pub mod nice;
pub mod pacer;
pub mod queue;
pub mod sub;
pub mod zero;
