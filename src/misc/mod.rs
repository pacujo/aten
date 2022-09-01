use std::io::{Result, Error};

use crate::{Disk, Fd};
use crate::stream::{ByteStream, file};

pub mod linger;
pub use linger::{Linger, WeakLinger};
pub mod duplex;
pub use duplex::{Duplex, WeakDuplex};
pub mod tcp_connect;
pub use tcp_connect::{TcpProgress, WeakTcpProgress};
pub mod unix_connect;
pub use unix_connect::{UnixProgress, WeakUnixProgress};
pub mod resolver;
pub use resolver::{Resolver, WeakResolver};

pub fn pipe(disk: &Disk) -> Result<(ByteStream, Fd)> {
    let mut pair = [0i32, 0i32];
    let status = unsafe {
        libc::pipe(&mut pair[0])
    };
    if status < 0 {
        Err(Error::last_os_error())
    } else {
        Ok((
            file::Stream::new(disk, &Fd::new(pair[0]), false)?.as_bytestream(),
            Fd::new(pair[1])
        ))
    }
}
