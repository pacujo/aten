use std::io::{Error, ErrorKind};

pub fn badf() -> Error {
    Error::from_raw_os_error(libc::EBADF)
}

pub fn again() -> Error {
    Error::from_raw_os_error(libc::EAGAIN)
}

pub fn inval() -> Error {
    Error::from_raw_os_error(libc::EINVAL)
}

pub fn proto() -> Error {
    Error::from_raw_os_error(libc::EPROTO)
}

pub fn nospc() -> Error {
    Error::from_raw_os_error(libc::ENOSPC)
}

pub fn is_again(err: &Error) -> bool {
    matches!(err.kind(), ErrorKind::WouldBlock)
}
