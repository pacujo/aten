use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, WeakDisk, Link, UID, Action, Registration, Fd};
use crate::{Downgradable, DECLARE_LINKS};
use crate::stream::{ByteStream, ByteStreamPair, ByteStreamPairBody};
use crate::stream::{DebuggableByteStreamPairBody, switch, file, dry, empty};
use crate::misc::Linger;
use r3::{TRACE, Traceable};

#[derive(Debug)]
pub struct DuplexBody {
    weak_disk: WeakDisk,
    uid: UID,
    ingress: Option<switch::Stream>,
    egress: Option<Linger>,
    eswitch: Option<switch::Stream>,
    registration: Option<Registration>,
}

impl DuplexBody {
    fn shut_down_ingress(&mut self) {
        if let Some(disk) = self.weak_disk.upgrade() {
            if let Some(switch) = self.ingress.take() {
                switch.switch(empty::Stream::new(&disk).as_bytestream());
            }
        }
    }

    fn shut_down_egress(&mut self) {
        if let Some(linger) = self.egress.take() {
            linger.abort();
        }
        self.eswitch.take();
    }

    fn notify(&self) {
        if let Some(inswitch) = &self.ingress {
            inswitch.invoke_callback();
        }
        if let Some(egress) = &self.egress {
            egress.prod();
        }
    }
} // impl DuplexBody

impl std::fmt::Display for DuplexBody {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.uid)
    }
} // impl std::fmt::Display for StreamBody

impl ByteStreamPairBody for DuplexBody {
    fn get_ingress(&self) -> Option<ByteStream> {
        self.ingress.as_ref().map(|s| s.as_bytestream())
    }

    fn set_egress(&mut self, egress: ByteStream) {
        if let Some(eswitch) = &self.eswitch {
            eswitch.switch(egress);
        }
    }
} // impl ByteStreamPairBody for DuplexBody

impl DebuggableByteStreamPairBody for DuplexBody {}

DECLARE_LINKS!(Duplex, WeakDuplex, DuplexBody, ATEN_DUPLEX_UPPED_MISS, DUPLEX);

impl Duplex {
    pub fn new(disk: &Disk, fd: &Fd) -> Result<Duplex> {
        let uid = UID::new();
        let eswitch = switch::Stream::new(
            &disk, dry::Stream::new(&disk).as_bytestream());
        let body = DuplexBody {
            weak_disk: disk.downgrade(),
            uid: uid,
            ingress: Some(switch::Stream::new(
                &disk, file::Stream::new(
                    &disk, &fd, true).unwrap().as_bytestream())),
            egress: Some(Linger::new(
                &disk, eswitch.as_bytestream(), &fd, true).unwrap()),
            eswitch: Some(eswitch),
            registration: None,
        };
        let duplex = Duplex(Link {
            uid: uid,
            body: Rc::new(RefCell::new(body)),
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
        TRACE!(ATEN_DUPLEX_CREATE { DISK: disk, DUPLEX: uid, FD: fd });
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
