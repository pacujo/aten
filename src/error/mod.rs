use std::io::Error;

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

pub fn is_again(err: &Error) -> bool {
    if let Some(errno) = err.raw_os_error() {
        errno == libc::EAGAIN
    } else {
        false
    }
}
