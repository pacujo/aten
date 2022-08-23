use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Result, Error};
use std::os::unix::io::RawFd;

use crate::{Disk, Link, UID, Registration};
use crate::stream::{ByteStreamBody, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM!(
    ATEN_FILESTREAM_DROP,
    ATEN_FILESTREAM_UPPED_MISS,
    ATEN_FILESTREAM_REGISTER_CALLBACK,
    ATEN_FILESTREAM_UNREGISTER_CALLBACK,
    ATEN_FILESTREAM_READ_TRIVIAL,
    ATEN_FILESTREAM_READ,
    ATEN_FILESTREAM_READ_DUMP,
    ATEN_FILESTREAM_READ_TEXT,
    ATEN_FILESTREAM_READ_FAIL);

#[derive(Debug)]
struct StreamBody {
    base: base::StreamBody,
    fd: RawFd,
    registration: Option<Registration>,
}

impl StreamBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        let count = unsafe {
            libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
        };
        if count < 0 {
            Err(Error::last_os_error())
        } else {
            Ok(count as usize)
        }
    }
}

impl Stream {
    IMPL_STREAM!();

    pub fn new(disk: &Disk, fd: RawFd, sync: bool) -> Result<Stream> {
        let uid = UID::new();
        let body = StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
            fd: fd,
            registration: None,
        };
        let stream = Stream(Link {
            uid: uid,
            body: Rc::new(RefCell::new(body)),
        });
        if !sync {
            let weak_stream = stream.downgrade();
            let notify = Rc::new(move || {
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
        if let Some(action) = self.get_callback() {
            (action)();
        }
    }
} // impl Stream
