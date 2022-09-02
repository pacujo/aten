use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Error, Result};
use std::os::unix::io::AsRawFd;

use crate::{Disk, WeakDisk, Link, UID, Action, Registration, Fd};
use crate::{Downgradable, error, DECLARE_LINKS};
use crate::stream::ByteStream;
use r3::{TRACE, Traceable};

#[derive(Debug)]
pub enum State {
    Busy,
    Drifting,
    Final(Result<()>),
    Stale,                      // Final result claimed previously
}

impl State {
    fn consume(&mut self) -> State {
        std::mem::replace(self, State::Stale)
    }
} // impl State

#[derive(Debug)]
pub struct LingerBody {
    weak_disk: WeakDisk,
    uid: UID,
    source: ByteStream,
    dest: Fd,
    buf: Vec<u8>,
    cursor: usize,
    length: usize,
    callback: Action,
    state: State,
    self_ref: Option<Rc<RefCell<LingerBody>>>,
    registration: Option<Registration>,
}

impl LingerBody {
    fn replenish(&mut self) -> Result<usize> {
        self.source.read(&mut self.buf)
    }

    fn done(&mut self, result: Result<()>) {
        TRACE!(ATEN_LINGER_JOCKEY_DONE { LINGER: self.uid });
        if matches!(self.state, State::Drifting) {
            self.consume();
        } else {
            self.state = State::Final(result);
            let weak_disk = &self.weak_disk;
            weak_disk.upped(|disk| { disk.execute(self.callback.clone()); });
        }
    }

    fn consume(&mut self) -> State {
        self.registration = None;
        self.self_ref = None;
        self.state.consume()
    }

    fn drift(&mut self) {
        match self.state {
            State::Busy => {
                TRACE!(ATEN_LINGER_DRIFT_BUSY { LINGER: self.uid });
                self.state = State::Drifting;
            }
            State::Drifting => {
                TRACE!(ATEN_LINGER_DRIFT_DRIFTING { LINGER: self.uid });
            }
            State::Stale => {
                TRACE!(ATEN_LINGER_DRIFT_STALE { LINGER: self.uid });
            }
            State::Final(_) => {
                TRACE!(ATEN_LINGER_DRIFT_FINAL { LINGER: self.uid });
                self.consume();
            }
        }
    }
} // impl LingerBody

impl std::fmt::Display for LingerBody {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.uid)
    }
} // impl std::fmt::Display for StreamBody


DECLARE_LINKS!(Linger, WeakLinger, LingerBody, ATEN_LINGER_UPPED_MISS, LINGER);

impl Linger {
    pub fn new(disk: &Disk, source: ByteStream, dest: &Fd, sync: bool)
               -> Result<Linger> {
        const BUF_SIZE: usize = 10000;
        let uid = UID::new();
        let body = LingerBody {
            weak_disk: disk.downgrade(),
            uid: uid,
            source: source.clone(),
            dest: dest.clone(),
            buf: vec![0; BUF_SIZE],
            cursor: 0,
            length: 0,
            callback: Action::noop(),
            state: State::Busy,
            self_ref: None,
            registration: None,
        };
        let self_ref = Rc::new(RefCell::new(body));
        self_ref.borrow_mut().self_ref = Some(self_ref.clone());
        let linger = Linger(Link {
            uid: uid,
            body: self_ref,
        });
        if !sync {
            let weak_linger = linger.downgrade();
            let jockey = Action::new(move || {
                weak_linger.upped(|linger| { linger.jockey(); });
            });
            match disk.register(dest, jockey) {
                Ok(registration) => {
                    linger.0.body.borrow_mut().registration =
                        Some(registration);
                }
                Err(err) => {
                    TRACE!(ATEN_LINGER_CREATE_FAIL {
                        DISK: disk, ERR: r3::errsym(&err)
                    });
                    return Err(err);
                }
            }
        }
        TRACE!(ATEN_LINGER_CREATE { DISK: disk, LINGER: uid, SYNC: sync });
        let weak_linger = linger.downgrade();
        source.register_callback(Action::new(move || {
            weak_linger.upped(|linger| { linger.jockey(); });
        }));
        Ok(linger)
    }

    pub fn register_callback(&self, callback: Action) {
        TRACE!(ATEN_LINGER_REGISTER_CALLBACK {
            LINGER: self, CALLBACK: &callback
        });
        self.0.body.borrow_mut().callback = callback;
    }

    pub fn unregister_callback(&self) {
        TRACE!(ATEN_LINGER_UNREGISTER_CALLBACK { LINGER: self });
        self.0.body.borrow_mut().callback = Action::noop();
    }

    pub fn poll(&self) -> State {
        let mut body = self.0.body.borrow_mut();
        match &body.state {
            State::Busy => {
                TRACE!(ATEN_LINGER_POLL_BUSY { LINGER: self });
                State::Busy
            }
            State::Drifting => {
                TRACE!(ATEN_LINGER_POLL_DRIFTING { LINGER: self });
                State::Drifting
            }
            State::Stale => {
                TRACE!(ATEN_LINGER_POLL_STALE { LINGER: self });
                State::Stale
            }
            State::Final(Err(err)) => {
                TRACE!(ATEN_LINGER_POLL_FAIL {
                    LINGER: self, ERR: r3::errsym(&err)
                });
                body.consume()
            }
            State::Final(_) => {
                TRACE!(ATEN_LINGER_POLL_FINAL { LINGER: self });
                body.consume()
            }
        }
    }

    pub fn abort(&self) -> State {
        let state = self.0.body.borrow_mut().consume();
        match &state {
            State::Busy => {
                TRACE!(ATEN_LINGER_ABORT_BUSY { LINGER: self });
            }
            State::Drifting => {
                TRACE!(ATEN_LINGER_ABORT_DRIFTING { LINGER: self });
            }
            State::Stale => {
                TRACE!(ATEN_LINGER_ABORT_STALE { LINGER: self });
            }
            State::Final(Err(err)) => {
                TRACE!(ATEN_LINGER_ABORT_FAIL {
                    LINGER: self, ERR: r3::errsym(&err)
                });
            }
            State::Final(_) => {
                TRACE!(ATEN_LINGER_ABORT_FINAL { LINGER: self });
            }
        }
        state
    }

    pub fn drift(&self) {
        self.0.body.borrow_mut().drift();
    }

    fn jockey(&self) {
        if !matches!(self.0.body.borrow().state, State::Busy) {
            TRACE!(ATEN_LINGER_JOCKEY_SPURIOUS { LINGER: self });
            return;
        }
        let mut body = self.0.body.borrow_mut();
        loop {
            while body.cursor < body.length {
                let slice = &body.buf[body.cursor..body.length];
                let count = unsafe {
                    libc::write(body.dest.as_raw_fd(),
                                slice.as_ptr() as *const libc::c_void,
                                slice.len())
                };
                if count < 0 {
                    let err = Error::last_os_error();
                    TRACE!(ATEN_LINGER_JOCKEY_WRITE_FAIL {
                        LINGER: self, WANT: slice.len(), ERR: r3::errsym(&err),
                    });
                    if !error::is_again(&err) {
                        body.done(Err(err));
                    }
                    return;
                }
                TRACE!(ATEN_LINGER_JOCKEY_WRITE {
                    LINGER: self, WANT: slice.len(), GOT: count,
                });
                TRACE!(ATEN_LINGER_JOCKEY_WRITE_DUMP {
                    LINGER: self, DATA: r3::octets(&slice[..count as usize]),
                });
                assert!(count > 0);
                body.cursor += count as usize;
            }
            match body.replenish() {
                Ok(count) => {
                    TRACE!(ATEN_LINGER_JOCKEY_REPLENISH {
                        LINGER: self, GOT: count,
                    });
                    if count == 0 {
                        body.done(Ok(()));
                        return;
                    }
                    body.cursor = 0;
                    assert!(count <= body.buf.len());
                    body.length = count;
                }
                Err(err) => {
                    TRACE!(ATEN_LINGER_JOCKEY_REPLENISH_FAIL {
                        LINGER: self, ERR: r3::errsym(&err),
                    });
                    if error::is_again(&err) {
                        body.cursor = body.length;
                    } else {
                        body.done(Err(err));
                    }
                    return;
                }
            }
        }
    }

    pub fn prod(&self) {
        let weak_linger = self.downgrade();
        let jockey = Action::new(move || {
            weak_linger.upped(|linger| { linger.jockey(); });
        });
        self.0.body.borrow().weak_disk.upped(|disk| {
            disk.execute(jockey.clone());
        });
    }
} // impl Linger
