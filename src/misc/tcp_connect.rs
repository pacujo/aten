use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Error, Result};
use std::net::SocketAddr;
use std::os::unix::io::{AsRawFd};

use crate::{Disk, WeakDisk, Link, UID, Action, Fd, Registration};
use crate::{Downgradable, nonblock, error, DECLARE_LINKS};
use crate::misc::duplex::Duplex;
use crate::stream::ByteStreamPair;
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
struct TcpProgressBody {
    weak_disk: WeakDisk,
    uid: UID,
    socket: Option<Fd>,
    state: State,
    registration: Option<Registration>,
    callback: Action,
}

impl TcpProgressBody {
    fn trigger(&mut self) {
        if matches!(self.state, State::InProgress) {
            self.state = State::Triggered;
            self.registration.take();
            self.weak_disk.upped(|disk| {
                TRACE!(ATEN_TCP_PROGRESS_TRIGGERED { PROGRESS: self.uid });
                disk.execute(self.callback.clone());
            });
        } else {
            TRACE!(ATEN_TCP_PROGRESS_TRIGGERED_SPURIOUSLY {
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
} // impl TcpProgressBody

DECLARE_LINKS!(TcpProgress, WeakTcpProgress, TcpProgressBody,
               ATEN_TCP_PROGRESS_UPPED_MISS, PROGRESS);

impl TcpProgress {
    pub fn new(disk: &Disk, address: &SocketAddr, action: Action)
               -> Result<TcpProgress> {
        let socket = Self::make_nonblocking_socket(disk, address)?;
        let result = try_connect(&socket, address);
        if matches!(result, Ok(())) {
            return TcpProgress::new_established(disk, address, action, socket);
        }
        let err = result.unwrap_err();
        if !is_inprogress(&err) {
            TRACE!(ATEN_TCP_PROGRESS_CREATE_CONNECT_FAIL {
                DISK: disk, ADDRESS: address, FD: &socket, ERR: &err,
            });
            return Err(err);
        }
        TcpProgress::new_in_progress(disk, address, action, socket)
    }

    fn make_nonblocking_socket(disk: &Disk, address: &SocketAddr) -> Result<Fd> {
        let family = match address {
            SocketAddr::V4(_) => libc::PF_INET,
            SocketAddr::V6(_) => libc::PF_INET6,
        };
        let skt = unsafe {
            libc::socket(family, libc::SOCK_STREAM, libc::IPPROTO_TCP)
        };
        if skt < 0 {
            let err = Error::last_os_error();
            TRACE!(ATEN_TCP_PROGRESS_CREATE_SOCKET_FAIL {
                DISK: disk, ADDRESS: address, ERR: &err
            });
            return Err(err);
        }
        let socket = Fd::new(skt);
        nonblock(&socket)?;
        Ok(socket)
    }

    fn new_established(disk: &Disk, address: &SocketAddr, action: Action,
                       socket: Fd)
                       -> Result<TcpProgress> {
        let uid = UID::new();
        let body = Rc::new(RefCell::new(TcpProgressBody {
            weak_disk: disk.downgrade(),
            uid: uid,
            socket: Some(socket.clone()),
            state: State::Established,
            registration: None,
            callback: Action::noop(),
        }));
        TRACE!(ATEN_TCP_PROGRESS_CREATE_ESTABLISHED {
            DISK: disk, PROGRESS: uid, ADDRESS: address, FD: &socket,
        });
        disk.execute(action);
        Ok(TcpProgress(Link {
            uid: uid,
            body: body,
        }))
    }

    fn new_in_progress(disk: &Disk, address: &SocketAddr, action: Action,
                       socket: Fd)
                       -> Result<TcpProgress> {
        let uid = UID::new();
        let body = TcpProgressBody {
            weak_disk: disk.downgrade(),
            uid: uid,
            socket: Some(socket.clone()),
            state: State::InProgress,
            registration: None,
            callback: action,
        };
        let progress = TcpProgress(Link {
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
            TRACE!(ATEN_TCP_PROGRESS_CREATE_REGISTER_FAIL {
                DISK: disk, ADDRESS: address, FD: &socket, ERR: &err,
            });
            return Err(err);
        }
        progress.0.body.borrow_mut().registration = Some(result.unwrap());
        TRACE!(ATEN_TCP_PROGRESS_CREATE_IN_PROGRESS {
            DISK: disk, PROGRESS: uid, ADDRESS: address, FD: &socket,
            ACTION: &progress.0.body.borrow().callback,
        });
        Ok(progress)
    }

    pub fn take(&self) -> Result<ByteStreamPair> {
        self.0.body.borrow_mut().take()
    }
} // impl TcpProgress

fn try_connect(socket: &Fd, address: &SocketAddr) -> Result<()> {
    let status = match address {
        SocketAddr::V4(v4) => {
            let addr_bytes = v4.ip().octets();
            let addr4 = libc::sockaddr_in {
                sin_family: libc::AF_INET as u16,
                sin_port: v4.port().to_be(),
                sin_addr: libc::in_addr {
                    s_addr: ((addr_bytes[0] as u32) << 24 |
                             (addr_bytes[1] as u32) << 16 |
                             (addr_bytes[2] as u32) << 8 |
                             addr_bytes[3] as u32).to_be(),
                },
                sin_zero: [0; 8],
            };
            unsafe {
                libc::connect(
                    socket.as_raw_fd(),
                    &addr4 as *const _ as *const libc::sockaddr,
                    std::mem::size_of_val(&addr4) as u32,
                )
            }
        }
        SocketAddr::V6(v6) => {
            let addr6 = libc::sockaddr_in6 {
                sin6_family: libc::AF_INET6 as u16,
                sin6_port: v6.port().to_be(),
                sin6_flowinfo: 0,
                sin6_addr: libc::in6_addr {
                    s6_addr: v6.ip().octets(),
                },
                sin6_scope_id: 0,
            };
            unsafe {
                libc::connect(
                    socket.as_raw_fd(),
                    &addr6 as *const _ as *const libc::sockaddr,
                    std::mem::size_of_val(&addr6) as u32,
                )
            }
        }
    };
    if status >= 0 {
        Ok(())
    } else {
        Err(Error::last_os_error())
    }
}

fn is_inprogress(err: &Error) -> bool {
    if let Some(errno) = err.raw_os_error() {
        errno == libc::EINPROGRESS
    } else {
        false
    }
}
