#![allow(dead_code)]

use std::cell::RefCell;
use std::io::{Result, Read};
use std::rc::Rc;

use crate::{Link, UID, DECLARE_LINKS};

DECLARE_LINKS!(ByteStream, WeakByteStream, dyn DebuggableByteStreamBody,
               ATEN_BYTESTREAM_UPPED_MISS, STREAM);

impl ByteStream {
    pub fn new(uid: UID, body: Rc<RefCell<dyn DebuggableByteStreamBody>>)
               -> ByteStream {
        ByteStream(Link {
            uid: uid,
            body: body,
        })
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        self.0.body.borrow_mut().read(buf)
    }

    pub fn register_callback(&self, callback: crate::Action) {
        self.0.body.borrow_mut().register_callback(callback);
    }

    pub fn unregister_callback(&self) {
        self.0.body.borrow_mut().unregister_callback();
    }
} // impl ByteStream

pub trait ByteStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    fn register_callback(&mut self, callback: crate::Action);
    fn unregister_callback(&mut self);
}

pub trait DebuggableByteStreamBody: ByteStreamBody + std::fmt::Debug {}

impl Read for ByteStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.0.body.borrow_mut().read(buf)
    }
}

DECLARE_LINKS!(ByteStreamPair, WeakByteStreamPair,
               dyn DebuggableByteStreamPairBody,
               ATEN_BYTESTREAM_PAIR_UPPED_MISS, STREAM_PAIR);

impl ByteStreamPair {
    pub fn new(uid: UID, body: Rc<RefCell<dyn DebuggableByteStreamPairBody>>)
               -> ByteStreamPair {
        ByteStreamPair(Link {
            uid: uid,
            body: body,
        })
    }

    pub fn get_ingress(&self) -> Option<ByteStream> {
        self.0.body.borrow().get_ingress()
    }

    pub fn set_egress(&self, egress: ByteStream) {
        self.0.body.borrow_mut().set_egress(egress);
    }
} // impl ByteStreamPair

pub trait ByteStreamPairBody {
    fn get_ingress(&self) -> Option<ByteStream>;
    fn set_egress(&mut self, egress: ByteStream);
}

pub trait DebuggableByteStreamPairBody: ByteStreamPairBody + std::fmt::Debug {}

#[macro_export]
macro_rules! DECLARE_STREAM {
    ($ATEN_STREAM_DROP:ident,
     $ATEN_STREAM_UPPED_MISS:ident,
     $ATEN_STREAM_REGISTER_CALLBACK:ident,
     $ATEN_STREAM_UNREGISTER_CALLBACK:ident,
     $ATEN_STREAM_READ_TRIVIAL:ident,
     $ATEN_STREAM_READ:ident,
     $ATEN_STREAM_READ_DUMP:ident,
     $ATEN_STREAM_READ_FAIL:ident) => {
        DECLARE_STREAM_DROP!($ATEN_STREAM_DROP);
        DECLARE_STREAM_NO_DROP!($ATEN_STREAM_UPPED_MISS,
                                $ATEN_STREAM_REGISTER_CALLBACK,
                                $ATEN_STREAM_UNREGISTER_CALLBACK,
                                $ATEN_STREAM_READ_TRIVIAL,
                                $ATEN_STREAM_READ,
                                $ATEN_STREAM_READ_DUMP,
                                $ATEN_STREAM_READ_FAIL);
    }
}

#[macro_export]
macro_rules! DECLARE_STREAM_DROP {
    ($ATEN_STREAM_DROP:ident) => {
        impl Drop for StreamBody {
            fn drop(&mut self) {
                TRACE!($ATEN_STREAM_DROP { STREAM: self });
            }
        }
    }
}

#[macro_export]
macro_rules! DECLARE_STREAM_NO_DROP {
    ($ATEN_STREAM_UPPED_MISS:ident,
     $ATEN_STREAM_REGISTER_CALLBACK:ident,
     $ATEN_STREAM_UNREGISTER_CALLBACK:ident,
     $ATEN_STREAM_READ_TRIVIAL:ident,
     $ATEN_STREAM_READ:ident,
     $ATEN_STREAM_READ_DUMP:ident,
     $ATEN_STREAM_READ_FAIL:ident) => {
        $crate::DECLARE_LINKS!(Stream, WeakStream, StreamBody,
                               $ATEN_STREAM_UPPED_MISS, STREAM);

        impl $crate::stream::ByteStreamBody for StreamBody {
            fn register_callback(&mut self, callback: $crate::Action) {
                TRACE!($ATEN_STREAM_REGISTER_CALLBACK {
                    STREAM: self, ACTION: &callback
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

        impl $crate::stream::DebuggableByteStreamBody for StreamBody {}

        impl From<Stream> for $crate::stream::ByteStream {
            fn from(stream: Stream) -> $crate::stream::ByteStream {
                stream.as_bytestream()
            }
        }
    }
}

#[macro_export]
macro_rules! IMPL_STREAM {
    () => {
        pub fn invoke_callback(&self) {
            self.0.body.borrow().base.invoke_callback();
        }

        pub fn as_bytestream(&self) -> $crate::stream::ByteStream {
            $crate::stream::ByteStream::new(self.0.uid, self.0.body.clone())
        }

        pub fn read(&self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.body.borrow_mut().read(buf)
        }

        pub fn register_callback(&self, callback: $crate::Action) {
            self.0.body.borrow_mut().register_callback(callback);
        }

        pub fn unregister_callback(&self) {
            self.0.body.borrow_mut().unregister_callback();
        }

        pub fn register_wrappee_callback(
            &self, wrappee: &$crate::stream::ByteStream) {
            let weak_stream = self.downgrade();
            let uid = self.0.uid;
            wrappee.register_callback($crate::Action::new(move || {
                match weak_stream.upgrade() {
                    Some(stream) => {
                        stream.invoke_callback();
                    }
                    None => {
                        r3::TRACE!(ATEN_STREAM_WRAPPEE_UPPED_MISS {
                            STREAM: uid
                        });
                    }
                }
            }));
        }
    }
}

pub mod avid;
pub mod base;
pub mod blob;
pub mod dry;
pub mod empty;
pub mod farewell;
pub mod file;
pub mod naivedecoder;
pub mod naiveencoder;
pub mod nice;
pub mod pacer;
pub mod queue;
pub mod reservoir;
pub mod sub;
pub mod switch;
pub mod zero;
