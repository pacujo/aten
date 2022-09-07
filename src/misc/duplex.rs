use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, Link, UID, Action, Registration, Fd};
use crate::{Downgradable, DECLARE_LINKS, IMPL_STREAM};
use crate::stream::{ByteStream, ByteStreamBody, DebuggableByteStreamBody};
use crate::stream::{ByteStreamPair, ByteStreamPairBody};
use crate::stream::{DebuggableByteStreamPairBody};
use crate::stream::{base, switch, file, dry};
use crate::misc::Linger;
use r3::{TRACE, Traceable};

#[derive(Debug)]
pub struct DuplexBody {
    base: base::StreamBody,
    weak_self: Weak<RefCell<DuplexBody>>,
    ingress: ByteStream,
    egress: Option<Linger>,
    eswitch: Option<switch::Stream>,
    registration: Option<Registration>,
}

impl DuplexBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.ingress.read(buf)
    }

    fn notify(&self) {
        self.base.invoke_callback();
        if let Some(egress) = &self.egress {
            egress.prod();
        }
    }
} // impl DuplexBody

impl ByteStreamBody for DuplexBody {
    fn register_callback(&mut self, callback: Action) {
        TRACE!(ATEN_DUPLEX_REGISTER_CALLBACK {
            STREAM: self, ACTION: &callback
        });
        self.base.register_callback(callback);
    }

    fn unregister_callback(&mut self) {
        TRACE!(ATEN_DUPLEX_UNREGISTER_CALLBACK { STREAM: self });
        self.base.unregister_callback();
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Ok(_) = self.base.read(buf) {
            TRACE!(ATEN_DUPLEX_READ_TRIVIAL { STREAM: self, WANT: buf.len() });
            return Ok(0);
        }
        match self.read_nontrivial(buf) {
            Ok(count) => {
                TRACE!(ATEN_DUPLEX_READ {
                    STREAM: self, WANT: buf.len(), GOT: count
                });
                TRACE!(ATEN_DUPLEX_READ_DUMP {
                    STREAM: self, DATA: r3::octets(&buf[..count])
                });
                Ok(count)
            }
            Err(err) => {
                TRACE!(ATEN_DUPLEX_READ_FAIL {
                    STREAM: self, WANT: buf.len(), ERR: r3::errsym(&err)
                });
                Err(err)
            }
        }
    }
}

impl std::fmt::Display for DuplexBody {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.base.get_uid())
    }
} // impl std::fmt::Display for StreamBody

impl DebuggableByteStreamBody for DuplexBody {}

impl ByteStreamPairBody for DuplexBody {
    fn get_ingress(&self) -> Option<ByteStream> {
        self.weak_self.upgrade().map(|s| ByteStream::new(self.base.get_uid(), s))
    }

    fn set_egress(&mut self, egress: ByteStream) {
        if let Some(eswitch) = &self.eswitch {
            TRACE!(ATEN_DUPLEX_SET_EGRESS { DUPLEX: self, EGRESS: egress });
            eswitch.switch(egress);
        }
    }
} // impl ByteStreamPairBody for DuplexBody

impl DebuggableByteStreamPairBody for DuplexBody {}

impl Drop for DuplexBody {
    fn drop(&mut self) {
        TRACE!(ATEN_DUPLEX_DROP { DUPLEX: self });
    }
}

DECLARE_LINKS!(Duplex, WeakDuplex, DuplexBody, ATEN_DUPLEX_UPPED_MISS, DUPLEX);

impl Duplex {
    IMPL_STREAM!();

    pub fn new(disk: &Disk, fd: &Fd) -> Result<Duplex> {
        let uid = UID::new();
        let eswitch = switch::Stream::new(
            &disk, dry::Stream::new(&disk).as_bytestream());
        let ingress = file::Stream::new(
            &disk, &fd, true).unwrap().as_bytestream();
        let egress = Linger::new(
            &disk, eswitch.as_bytestream(), &fd, true).unwrap();
        let body = Rc::new_cyclic(
            |weak_self| RefCell::new(
                DuplexBody {
                    base: base::StreamBody::new(disk.downgrade(), uid),
                    weak_self: weak_self.clone(),
                    ingress: ingress.clone(),
                    egress: Some(egress.clone()),
                    eswitch: Some(eswitch),
                    registration: None,
                }
            ));
        let duplex = Duplex(Link {
            uid: uid,
            body: body,
        });
        let weak_duplex = duplex.downgrade();
        let notify = Action::new(move || {
            weak_duplex.upped(|duplex| { duplex.notify(); });
        });
        match disk.register(fd, notify) {
            Ok(registration) => {
                duplex.0.body.borrow_mut().registration =
                    Some(registration);
            }
            Err(err) => {
                TRACE!(ATEN_FILEDUPLEX_CREATE_FAIL {
                    DISK: disk, ERR: r3::errsym(&err)
                });
                return Err(err);
            }
        }
        TRACE!(ATEN_DUPLEX_CREATE {
            DISK: disk, DUPLEX: uid, FD: fd, INGRESS: ingress,
            EGRESS_LINGER: egress,
        });
        Ok(duplex)
    }

    pub fn get_ingress(&self) -> Option<ByteStream> {
        self.0.body.borrow().get_ingress()
    }

    pub fn set_egress(&self, egress: ByteStream) {
        self.0.body.borrow_mut().set_egress(egress);
    }

    pub fn as_bytestream_pair(&self) -> ByteStreamPair {
        ByteStreamPair::new(self.0.uid, self.0.body.clone())
    }

    fn notify(&self) {
        self.0.body.borrow().notify();
    }
} // impl Duplex
