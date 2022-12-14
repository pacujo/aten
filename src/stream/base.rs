use std::io::Result;

use crate::{WeakDisk, UID, Action, Upgradable, error};
use crate::stream::{ByteStreamBody};
use r3::{TRACE, Traceable};

#[derive(Debug)]
pub struct StreamBody {
    weak_disk: WeakDisk,
    uid: UID,
    callback: Action,
}

impl std::fmt::Display for StreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.uid)
    }
} // impl std::fmt::Display for StreamBody

impl StreamBody {
    pub fn new(weak_disk: WeakDisk, uid: UID) -> StreamBody {
        StreamBody {
            weak_disk: weak_disk,
            uid: uid,
            callback: Action::noop(),
        }
    }

    pub fn get_weak_disk(&self) -> &WeakDisk {
        &self.weak_disk
    }

    pub fn get_uid(&self) -> UID {
        self.uid
    }

    pub fn invoke_callback(&self) {
        self.weak_disk.upped(|disk| {
            TRACE!(ATEN_BASESTREAM_INVOKE_CALLBACK { STREAM: self.uid });
            disk.execute(self.callback.clone());
        });
    }
} // impl StreamBody

impl ByteStreamBody for StreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.len() == 0 {
            Ok(0)
        } else {
            Err(error::again())
        }
    }

    fn register_callback(&mut self, callback: Action) {
        self.get_weak_disk().upped(
            |disk| { disk.execute(callback.clone()); }
        );
        self.callback = callback;
    }

    fn unregister_callback(&mut self) {
        self.callback = Action::noop();
    }
} // impl ByteStreamBody for StreamBody
