use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Error, Result};
use std::os::unix::io::{AsRawFd};

use crate::{Disk, WeakDisk, Link, UID, Action, Fd, Registration};
use crate::{Downgradable, nonblock, error, DECLARE_LINKS};
use crate::stream::ByteStreamPair;
use crate::misc::duplex::Duplex;
use r3::{TRACE, Traceable};

#[derive(Debug)]
enum State {
    InProgress,
    Triggered,
    Established,
    Done,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
} // impl std::fmt::Display for State

#[derive(Debug)]
struct UnixProgressBody {
    weak_disk: WeakDisk,
    uid: UID,
    socket: Option<Fd>,
    state: State,
    registration: Option<Registration>,
    callback: Action,
}

impl UnixProgressBody {
    fn trigger(&mut self) {
        if matches!(self.state, State::InProgress) {
            self.state = State::Triggered;
            self.registration.take();
            if let Some(disk) = self.weak_disk.upgrade() {
                TRACE!(ATEN_UNIX_PROGRESS_TRIGGERED { PROGRESS: self.uid });
                disk.execute(self.callback.clone());
            };
        } else {
            TRACE!(ATEN_UNIX_PROGRESS_TRIGGERED_SPURIOUSLY {
                PROGRESS: self.uid, STATE: &self.state,
            });
        }
    }

    fn take(&mut self) -> Result<ByteStreamPair> {
        match self.state {
            State::InProgress => {
                Err(error::again())
            }
            State::Triggered => {
                self.state = State::Done;
                self.get_socket_status()
            }
            State::Established => {
                self.state = State::Done;
                match self.weak_disk.upgrade() {
                    Some(disk) => {
                        Duplex::new(&disk, &self.socket.take().unwrap())
                            .map(|dup| dup.as_bytestream_pair())
                    }
                    None => {
                        Err(error::badf())
                    }
                }
            }
            State::Done => {
                Err(error::badf()) // already handed off
            }
        }
    }

    fn get_socket_status(&mut self) -> Result<ByteStreamPair> {
        let mut errno = 0i32;
        let mut errlen = std::mem::size_of_val(&errno) as libc::socklen_t;
        let status = unsafe {
            libc::getsockopt(self.socket.as_ref().unwrap().as_raw_fd(),
                             libc::SOL_SOCKET, libc::SO_ERROR,
                             &mut errno as *mut _ as *mut libc::c_void,
                             &mut errlen as *mut u32)
        };
        if status < 0 {
            return Err(Error::last_os_error());
        }
        match errno {
            0 => {
                match self.weak_disk.upgrade() {
                    Some(disk) => {
                        Duplex::new(&disk, &self.socket.take().unwrap())
                            .map(|dup| dup.as_bytestream_pair())
                    }
                    None => {
                        Err(error::badf())
                    }
                }
            }
            libc::EINPROGRESS | libc::EAGAIN => {
                unreachable!();
            }
            _ => {
                Err(Error::from_raw_os_error(errno))
            }
        }
    }
} // impl UnixProgressBody

DECLARE_LINKS!(UnixProgress, WeakUnixProgress, UnixProgressBody,
               ATEN_UNIX_PROGRESS_UPPED_MISS, PROGRESS);

impl UnixProgress {
    pub fn new(disk: &Disk, address: &std::path::Path, action: Action)
               -> Result<UnixProgress> {
        let socket = Self::make_nonblocking_socket(disk, address)?;
        let result = try_connect(&socket, address);
        if matches!(result, Ok(())) {
            return UnixProgress::new_established(disk, address, action, socket);
        }
        let err = result.unwrap_err();
        if !is_inprogress(&err) {
            TRACE!(ATEN_UNIX_PROGRESS_CREATE_CONNECT_FAIL {
                DISK: disk, ADDRESS: address.to_string_lossy(),
                FD: &socket, ERR: &err,
            });
            return Err(err);
        }
        UnixProgress::new_in_progress(disk, address, action, socket)
    }

    fn make_nonblocking_socket(disk: &Disk, address: &std::path::Path)
                               -> Result<Fd> {
        let skt = unsafe {
            libc::socket(libc::PF_UNIX, libc::SOCK_STREAM, 0)
        };
        if skt < 0 {
            let err = Error::last_os_error();
            TRACE!(ATEN_UNIX_PROGRESS_CREATE_SOCKET_FAIL {
                DISK: disk, ADDRESS: address.to_string_lossy(), ERR: &err
            });
            return Err(err);
        }
        let socket = Fd::new(skt);
        nonblock(&socket)?;
        Ok(socket)
    }

    fn new_established(disk: &Disk, address: &std::path::Path, action: Action,
                       socket: Fd)
                       -> Result<UnixProgress> {
        let uid = UID::new();
        let body = Rc::new(RefCell::new(UnixProgressBody {
            weak_disk: disk.downgrade(),
            uid: uid,
            socket: Some(socket.clone()),
            state: State::Established,
            registration: None,
            callback: Action::noop(),
        }));
        TRACE!(ATEN_UNIX_PROGRESS_CREATE_ESTABLISHED {
            DISK: disk, PROGRESS: uid, ADDRESS: address.to_string_lossy(),
            FD: &socket,
        });
        disk.execute(action);
        Ok(UnixProgress(Link {
            uid: uid,
            body: body,
        }))
    }

    fn new_in_progress(disk: &Disk, address: &std::path::Path, action: Action,
                       socket: Fd)
                       -> Result<UnixProgress> {
        let uid = UID::new();
        let body = UnixProgressBody {
            weak_disk: disk.downgrade(),
            uid: uid,
            socket: Some(socket.clone()),
            state: State::InProgress,
            registration: None,
            callback: action,
        };
        let progress = UnixProgress(Link {
            uid: uid,
            body: Rc::new(RefCell::new(body)),
        });
        let weak_progress = progress.downgrade();
        let result = disk.register(&socket, Action::new(move || {
            weak_progress.upped(|progress| {
                progress.0.body.borrow_mut().trigger();
            });
        }));
        if let Err(err) = result {
            TRACE!(ATEN_UNIX_PROGRESS_CREATE_REGISTER_FAIL {
                DISK: disk, ADDRESS: address.to_string_lossy(), FD: &socket,
                ERR: &err,
            });
            return Err(err);
        }
        progress.0.body.borrow_mut().registration = Some(result.unwrap());
        TRACE!(ATEN_UNIX_PROGRESS_CREATE_IN_PROGRESS {
            DISK: disk, PROGRESS: uid, ADDRESS: address.to_string_lossy(),
            FD: &socket, ACTION: &progress.0.body.borrow().callback,
        });
        Ok(progress)
    }

    pub fn take(&self) -> Result<ByteStreamPair> {
        self.0.body.borrow_mut().take()
    }
} // impl UnixProgress

fn try_connect(socket: &Fd, address: &std::path::Path) -> Result<()> {
    let sockaddr = std::os::unix::net::SocketAddr::from_pathname(address)?;
    let status = unsafe {
        libc::connect(
            socket.as_raw_fd(),
            &sockaddr as *const _ as *const libc::sockaddr,
            std::mem::size_of_val(&sockaddr) as u32,
        )
    };
    if status >= 0 {
        Ok(())
    } else {
        Err(Error::last_os_error())
    }
}

fn is_inprogress(err: &Error) -> bool {
    error::is_again(err)
}

pub fn socket_pair(disk: &Disk)
                   -> Result<((ByteStreamPair, Fd), (ByteStreamPair, Fd))> {
    let mut pair = [0i32, 0i32];
    let status = unsafe {
        libc::socketpair(libc::PF_UNIX, libc::SOCK_STREAM, 0, &mut pair[0])
    };
    if status < 0 {
        Err(Error::last_os_error())
    } else {
        let fd0 = Fd::new(pair[0]);
        let fd1 = Fd::new(pair[1]);
        Ok(((Duplex::new(disk, &fd0)?.as_bytestream_pair(), fd0),
            (Duplex::new(disk, &fd1)?.as_bytestream_pair(), fd1)))
    }
}
