use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Result, Error};
use std::os::unix::io::AsRawFd;

use crate::{Disk, Link, Action, UID, Registration, Fd, Downgradable, Upgradable};
use crate::stream::{BasicStream, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    Stream, WeakStream, StreamBody,
    ATEN_FILESTREAM_DROP,
    ATEN_FILESTREAM_UPPED_MISS,
    ATEN_FILESTREAM_REGISTER_CALLBACK,
    ATEN_FILESTREAM_UNREGISTER_CALLBACK,
    ATEN_FILESTREAM_READ_TRIVIAL,
    ATEN_FILESTREAM_READ,
    ATEN_FILESTREAM_READ_DUMP,
    ATEN_FILESTREAM_READ_FAIL);

#[derive(Debug)]
pub struct StreamBody {
    base: base::StreamBody,
    fd: Fd,
    registration: Option<Registration>,
}

impl StreamBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        let count = unsafe {
            libc::read(self.fd.as_raw_fd(),
                       buf.as_mut_ptr() as *mut libc::c_void, buf.len())
        };
        if count < 0 {
            Err(Error::last_os_error())
        } else {
            Ok(count as usize)
        }
    }
}

impl Stream {
    pub fn new(disk: &Disk, fd: &Fd, sync: bool) -> Result<Stream> {
        let uid = UID::new();
        let body = StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
            fd: fd.clone(),
            registration: None,
        };
        let stream = Stream(Link {
            uid: uid,
            body: Rc::new(RefCell::new(body)),
        });
        if !sync {
            let weak_stream = stream.downgrade();
            let notify = Action::new(move || {
                weak_stream.upped(|stream| { stream.notify(); });
            });
            match disk.register(fd, notify) {
                Ok(registration) => {
                    stream.0.body.borrow_mut().registration =
                        Some(registration);
                }
                Err(err) => {
                    TRACE!(ATEN_FILESTREAM_CREATE_FAIL {
                        DISK: disk, ERR: r3::errsym(&err)
                    });
                    return Err(err);
                }
            }
        }
        TRACE!(ATEN_FILESTREAM_CREATE { DISK: disk, STREAM: uid, SYNC: sync });
        Ok(stream)
    }

    fn notify(&self) {
        self.0.body.borrow().base.invoke_callback();
    }
} // impl Stream
