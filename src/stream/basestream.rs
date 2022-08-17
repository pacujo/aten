use std::io::Result;

use crate::{WeakDisk, UID, Action, again};
use crate::stream::{ByteStreamBody};

pub struct BaseStreamBody {
    weak_disk: WeakDisk,
    uid: UID,
    callback: Option<Action>,
}

impl std::fmt::Display for BaseStreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.uid)
    }
} // impl std::fmt::Display for BaseStreamBody

impl BaseStreamBody {
    pub fn new(weak_disk: WeakDisk, uid: UID) -> BaseStreamBody {
        BaseStreamBody {
            weak_disk: weak_disk,
            uid: uid,
            callback: None,
        }
    }

    pub fn get_weak_disk(&self) -> &WeakDisk {
        &self.weak_disk
    }

    pub fn get_uid(&self) -> UID {
        self.uid
    }

    pub fn get_callback(&self) -> Option<Action> {
        self.callback.clone()
    }
} // impl BaseStreamBody

impl ByteStreamBody for BaseStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.len() == 0 {
            Ok(0)
        } else {
            Err(again())
        }
    }

    fn register(&mut self, callback: Option<Action>) {
        self.callback = callback;
    }
} // impl ByteStreamBody for BaseStreamBody

impl std::fmt::Debug for BaseStreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BaseStreamBody")
            .field("weak_disk", &self.weak_disk)
            .field("uid", &self.uid)
            .field("callback", &self.callback.is_some())
            .finish()
    }
} // impl Debug for BaseStreamBody 

