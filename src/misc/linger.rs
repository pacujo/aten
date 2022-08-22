use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Error, Result};
use std::os::unix::io::RawFd;

use crate::{Disk, WeakDisk, Link, WeakLink, UID, Action};
use crate::{is_again, action_to_string, callback_to_string};
use crate::stream::ByteStream;
use r3::TRACE;

#[derive(Debug)]
pub enum State {
    Busy,
    Final(Result<()>),
    Stale,                      // Final result claimed previously
}

impl State {
    fn consume(&mut self) -> State {
        std::mem::replace(self, State::Stale)
    }
} // impl State

pub struct LingerBody {
    weak_disk: WeakDisk,
    uid: UID,
    source: ByteStream,
    dest: RawFd,
    buf: Vec<u8>,
    cursor: usize,
    length: usize,
    callback: Option<Action>,
    state: State,
    self_ref: Option<Rc<RefCell<LingerBody>>>,
}

impl LingerBody {
    fn replenish(&mut self) -> Result<usize> {
        self.source.read(&mut self.buf)
    }

    fn done(&mut self, result: Result<()>) {
        TRACE!(ATEN_LINGER_JOCKEY_DONE { LINGER: self.uid });
        self.state = State::Final(result);
        if let Some(action) = self.callback.clone() {
            let weak_disk = &self.weak_disk;
            weak_disk.upped(|disk| { disk.execute(action.clone()); });
        }
    }

    fn consume(&mut self) -> State{
        self.weak_disk.upped(|disk| {
            disk.unregister(self.dest).unwrap();
        });
        self.self_ref = None;
        self.state.consume()
    }
} // impl LingerBody

impl std::fmt::Display for LingerBody {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.uid)
    }
} // impl std::fmt::Display for StreamBody

impl std::fmt::Debug for LingerBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LingerBody")
            .field("uid", &self.uid)
            .field("source", &self.source)
            .field("dest", &self.dest)
            .field("buf", &self.buf)
            .field("cursor", &self.cursor)
            .field("length", &self.length)
            .field("callback", &callback_to_string(&self.callback))
            .field("state", &self.state)
            .finish()
    }
}

#[derive(Debug)]
pub struct Linger(Link<LingerBody>);

impl Linger {
    pub fn new(disk: &Disk, source: ByteStream, dest: RawFd)
               -> Result<Linger> {
        const BUF_SIZE: usize = 10000;
        let uid = UID::new();
        let body = LingerBody {
            weak_disk: disk.downgrade(),
            uid: uid,
            source: source,
            dest: dest,
            buf: vec![0; BUF_SIZE],
            cursor: 0,
            length: 0,
            callback: None,
            state: State::Busy,
            self_ref: None,
        };
        let self_ref = Rc::new(RefCell::new(body));
        self_ref.borrow_mut().self_ref = Some(self_ref.clone());
        let linger = Linger(Link {
            uid: uid,
            body: self_ref,
        });
        {
            let weak_linger = linger.downgrade();
            if let Err(err) = disk.register(dest, Rc::new(move || {
                weak_linger.upped(|linger| { linger.jockey(); });
            })) {
                TRACE!(ATEN_LINGER_CREATE_FAIL {
                    DISK: disk, ERR: r3::errsym(&err)
                });
                return Err(err);
            }
        }
        TRACE!(ATEN_LINGER_CREATE { DISK: disk, LINGER: uid });
        {
            let weak_linger = linger.downgrade();
            disk.execute(Rc::new(move || {
                weak_linger.upped(|linger| { linger.jockey(); });
            }));
        }
        Ok(linger)
    }

    pub fn downgrade(&self) -> WeakLinger {
        WeakLinger(WeakLink {
            uid: self.0.uid,
            body: Rc::downgrade(&self.0.body),
        })
    }

    pub fn register_callback(&self, callback: Action) {
        TRACE!(ATEN_LINGER_REGISTER_CALLBACK {
            LINGER: self.0.uid, CALLBACK: action_to_string(&callback)
        });
        self.0.body.borrow_mut().callback = Some(callback);
    }

    pub fn unregister_callback(&self) {
        self.0.body.borrow_mut().callback = None;
    }

    pub fn poll(&self) -> State {
        let mut body = self.0.body.borrow_mut();
        match &body.state {
            State::Busy => {
                TRACE!(ATEN_LINGER_POLL_BUSY { LINGER: self.0.uid });
                State::Busy
            }
            State::Stale => {
                TRACE!(ATEN_LINGER_POLL_STALE { LINGER: self.0.uid });
                State::Stale
            }
            State::Final(Err(err)) => {
                TRACE!(ATEN_LINGER_POLL_FAIL {
                    LINGER: self.0.uid, ERR: r3::errsym(&err)
                });
                body.consume()
            }
            State::Final(_) => {
                TRACE!(ATEN_LINGER_POLL_FINAL { LINGER: self.0.uid });
                body.consume()
            }
        }
    }

    fn errsym(result: &Result<()>) -> String {
        if let Err(err) = result {
            r3::errsym(&err)
        } else {
            "".to_string()
        }
    }

    fn jockey(&self) {
        if self.is_done() {
            TRACE!(ATEN_LINGER_JOCKEY_SPURIOUS { LINGER: self.0.uid });
            return;
        }
        let mut body = self.0.body.borrow_mut();
        loop {
            while body.cursor < body.length {
                let slice = &body.buf[body.cursor..body.length];
                let count = unsafe {
                    libc::write(body.dest,
                                slice.as_ptr() as *const libc::c_void,
                                slice.len())
                };
                if count < 0 {
                    let err = Error::last_os_error();
                    TRACE!(ATEN_LINGER_JOCKEY_WRITE_FAIL {
                        LINGER: self.0.uid, WANT: slice.len(),
                        ERR: r3::errsym(&err),
                    });
                    if !is_again(&err) {
                        body.done(Err(err));
                    }
                    return;
                }
                TRACE!(ATEN_LINGER_JOCKEY_WRITE {
                    LINGER: self.0.uid, WANT: slice.len(), GOT: count,
                });
                TRACE!(ATEN_LINGER_JOCKEY_WRITE_DUMP {
                    LINGER: self.0.uid,
                    DATA: r3::octets(&slice[..count as usize]),
                });
                assert!(count > 0);
                body.cursor += count as usize;
            }
            match body.replenish() {
                Ok(count) => {
                    TRACE!(ATEN_LINGER_JOCKEY_REPLENISH {
                        LINGER: self.0.uid, GOT: count,
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
                        LINGER: self.0.uid, ERR: r3::errsym(&err),
                    });
                    if is_again(&err) {
                        body.cursor = body.length;
                    } else {
                        body.done(Err(err));
                    }
                    return;
                }
            }
        }
    }

    fn is_done(&self) -> bool {
        if let State::Busy = self.0.body.borrow().state {
            false
        } else {
            true
        }
    }
} // impl Linger

#[derive(Debug)]
pub struct WeakLinger(WeakLink<LingerBody>);

impl WeakLinger {
    pub fn upgrade(&self) -> Option<Linger> {
        self.0.body.upgrade().map(|body|
            Linger(Link {
                uid: self.0.uid,
                body: body,
            }))
    }

    pub fn upped<F>(&self, f: F) -> Option<()> where F: Fn(&Linger) {
        match self.upgrade() {
            Some(linger) => {
                f(&linger);
                Some(())
            }
            None => {
                TRACE!(ATEN_LINGER_UPPED_MISS { LINGER: self.0.uid });
                None
            }
        }
    }
} // impl WeakLinger

