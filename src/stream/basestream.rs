use std::io::Result;

use crate::{WeakDisk, UID, Action, again};
use crate::stream::{ByteStreamBody};

pub struct BaseStreamBody {
    weak_disk: WeakDisk,
    uid: UID,
    closed: bool,
    callback: Option<Action>,
}

impl BaseStreamBody {
    pub fn new(weak_disk: WeakDisk, uid: UID) -> BaseStreamBody {
        BaseStreamBody {
            weak_disk: weak_disk,
            uid: uid,
            closed: false,
            callback: None,
        }
    }

    pub fn get_weak_disk(&self) -> &WeakDisk {
        &self.weak_disk
    }

    pub fn get_uid(&self) -> UID {
        self.uid
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.closed || buf.len() == 0 {
            Ok(0)
        } else {
            Err(again())
        }
    }

    pub fn close(&mut self) {
        self.closed = true;
    }

    pub fn register(&mut self, callback: Option<Action>) {
        self.callback = callback;
    }

    pub fn get_callback(&self) -> Option<Action> {
        self.callback.clone()
    }
} // impl BaseStreamBody

impl ByteStreamBody for BaseStreamBody {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.closed || buf.len() == 0 {
            Ok(0)
        } else {
            Err(again())
        }
    }

    fn close(&mut self) {
        self.closed = true;
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
            .field("closed", &self.closed)
            .field("callback", &self.callback.is_some())
            .finish()
    }
} // impl Debug for BaseStreamBody 

