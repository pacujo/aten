use std::io::Result;
use std::net::{SocketAddr, ToSocketAddrs};
use std::rc::Rc;
use std::cell::RefCell;
use std::thread::JoinHandle;

use crate::{Disk, WeakDisk, UID, Action, Link, Downgradable, error};
use crate::DECLARE_LINKS;
use crate::stream::ByteStream;
use crate::misc::pipe;
use r3::{TRACE, TRACE_ENABLED, Traceable};

#[derive(Debug)]
struct ResolverBody {
    weak_disk: WeakDisk,
    uid: UID,
    pipe: ByteStream,
    jh: Option<JoinHandle<Result<std::vec::IntoIter<SocketAddr>>>>,
    callback: Action,
}

DECLARE_LINKS!(Resolver, WeakResolver, ResolverBody,
               ATEN_LINGER_UPPED_MISS, RESOLVER);

impl Resolver {
    pub fn new(disk: &Disk, name: String) -> Result<Resolver> {
        match pipe(disk) {
            Ok((read_stream, write_fd)) => {
                let uid = UID::new();
                TRACE!(ATEN_RESOLVER_CREATE { RESOLVER: uid, NAME: name });
                let body = ResolverBody {
                    weak_disk: disk.downgrade(),
                    uid: uid,
                    pipe: read_stream.clone(),
                    jh: Some(std::thread::spawn(move || {
                        let _thread_termination_sentinel = write_fd;
                        name.to_socket_addrs()
                    })),
                    callback: Action::noop(),
                };
                let resolver = Resolver(Link {
                    uid: uid,
                    body: Rc::new(RefCell::new(body)),
                });
                let weak_resolver = resolver.downgrade();
                read_stream.register_callback(Action::new(move || {
                    weak_resolver.upped(|resolver| {
                        let body = resolver.0.body.borrow();
                        body.weak_disk.upped(|disk| {
                            disk.execute(body.callback.clone());
                        });
                    });
                }));
                Ok(resolver)
            }
            Err(err) => {
                TRACE!(ATEN_RESOLVER_CREATE_FAIL {
                    NAME: name, ERR: r3::errsym(&err)
                });
                Err(err)
            }
        }
    }

    pub fn register_callback(&self, callback: Action) {
        TRACE!(ATEN_RESOLVER_REGISTER_CALLBACK {
            RESOLVER: self, CALLBACK: &callback
        });
        self.0.body.borrow_mut().callback = callback;
    }

    pub fn unregister_callback(&self) {
        TRACE!(ATEN_RESOLVER_UNREGISTER_CALLBACK { RESOLVER: self });
        self.0.body.borrow_mut().callback = Action::noop();
    }

    pub fn poll(&self) -> Result<std::vec::IntoIter<SocketAddr>> {
        let mut body = self.0.body.borrow_mut();
        let mut buffer = [0u8];
        match body.pipe.read(&mut buffer) {
            Ok(0) => {
                match body.jh.take() {
                    None => {
                        TRACE!(ATEN_RESOLVER_POLL_SPURIOUS { RESOLVER: self });
                        Err(error::badf())
                    }
                    Some(jh) => {
                        let result = jh.join().unwrap();
                        if TRACE_ENABLED!(ATEN_RESOLVER_POLL_RESOLVED) {
                            match &result {
                                Ok(addresses) => {
                                    for address in addresses.clone() {
                                        TRACE!(ATEN_RESOLVER_POLL_RESOLVED {
                                            RESOLVER: self, ADDRESS: address,
                                        });
                                    }
                                }
                                Err(err) => {
                                    TRACE!(ATEN_RESOLVER_POLL_JOIN_FAIL {
                                        RESOLVER: self, ERR: r3::errsym(&err)
                                    });
                                }
                            }
                        }
                        result
                    }
                }
            }
            Ok(_) => {
                unreachable!()
            }
            Err(err) => {
                TRACE!(ATEN_RESOLVER_POLL_READ_FAIL {
                    RESOLVER: self, ERR: r3::errsym(&err)
                });
                Err(err)
            }
        }
    }
} // impl Resolver
