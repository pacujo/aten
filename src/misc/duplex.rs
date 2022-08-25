use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, WeakDisk, Link, WeakLink, UID, Action, Registration, Fd};
use crate::stream::{ByteStream, switch, file, dry, empty};
use crate::misc::{Linger};
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

#[derive(Debug)]
pub struct Duplex(Link<DuplexBody>);

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

    pub fn downgrade(&self) -> WeakDuplex {
        WeakDuplex(WeakLink {
            uid: self.0.uid,
            body: Rc::downgrade(&self.0.body),
        })
    }

    pub fn get_ingress(&self) -> Option<ByteStream> {
        self.0.body.borrow().ingress.as_ref().map(|s| s.as_bytestream())
    }

    pub fn set_egress(&self, egress: ByteStream) {
        if let Some(eswitch) = &self.0.body.borrow_mut().eswitch {
            eswitch.switch(egress);
        }
    }

    fn notify(&self) {
        self.0.body.borrow().notify();
    }
} // impl Duplex

#[derive(Debug)]
pub struct WeakDuplex(WeakLink<DuplexBody>);

impl WeakDuplex {
    pub fn upgrade(&self) -> Option<Duplex> {
        self.0.body.upgrade().map(|body|
            Duplex(Link {
                uid: self.0.uid,
                body: body,
            }))
    }

    pub fn upped<F, R>(&self, f: F) -> Option<R> where F: Fn(&Duplex) -> R {
        match self.upgrade() {
            Some(duplex) => Some(f(&duplex)),
            None => {
                TRACE!(ATEN_DUPLEX_UPPED_MISS { DUPLEX: self.0.uid });
                None
            }
        }
    }
} // impl WeakDuplex

