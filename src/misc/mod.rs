pub mod linger;
pub use linger::{Linger, WeakLinger};
pub mod duplex;
pub use duplex::{Duplex, WeakDuplex};
pub mod tcp_connect;
pub use tcp_connect::{TcpProgress, WeakTcpProgress};
pub mod unix_connect;
pub use unix_connect::{UnixProgress, WeakUnixProgress};
