use std::rc::Rc;
use std::cell::RefCell;
use std::io::Result;

use crate::{Disk, Link, UID, Action, action_to_string};
use crate::stream::{ByteStream, ByteStreamBody, base};
use r3::{TRACE, Traceable};

DECLARE_STREAM_NO_DROP!(
    ATEN_FAREWELLSTREAM_UPPED_MISS,
    ATEN_FAREWELLSTREAM_REGISTER_CALLBACK,
    ATEN_FAREWELLSTREAM_UNREGISTER_CALLBACK,
    ATEN_FAREWELLSTREAM_READ_TRIVIAL,
    ATEN_FAREWELLSTREAM_READ,
    ATEN_FAREWELLSTREAM_READ_DUMP,
    ATEN_FAREWELLSTREAM_READ_TEXT,
    ATEN_FAREWELLSTREAM_READ_FAIL);

pub struct StreamBody {
    base: base::StreamBody,
    wrappee: ByteStream,
    farewell_callback: Option<Action>,
}

impl std::fmt::Debug for StreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("farewell::StreamBody")
            .field("base", &self.base)
            .field("wrappee", &self.wrappee)
            .field("farewell_callback", &self.farewell_callback.is_some())
            .finish()
    }
} // impl Debug for StreamBody 

impl StreamBody {
    fn read_nontrivial(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.wrappee.read(buf)
    }
} // impl StreamBody

impl Drop for StreamBody {
    fn drop(&mut self) {
        TRACE!(ATEN_FAREWELLSTREAM_DROP { STREAM: self });
        if let Some(action) = &self.farewell_callback {
            self.base.get_weak_disk().upped(|disk| {
                disk.execute(action.clone());
            });
        }
    }
} // impl Drop for StreamBody

impl Stream {
    IMPL_STREAM!();

    pub fn new(disk: &Disk, wrappee: ByteStream) -> Stream {
        let uid = UID::new();
        TRACE!(ATEN_FAREWELLSTREAM_CREATE {
            DISK: disk, STREAM: uid, WRAPPEE: wrappee,
        });
        let body = Rc::new(RefCell::new(StreamBody {
            base: base::StreamBody::new(disk.downgrade(), uid),
            wrappee: wrappee.clone(),
            farewell_callback: None,
        }));
        let stream = Stream(Link {
            uid: uid,
            body: body.clone(),
        });
        stream.register_wrappee_callback(&wrappee);
        stream
    }

    pub fn register_farewell_callback(&self, callback: Action) {
        TRACE!(ATEN_FAREWELLSTREAM_REGISTER_FAREWELL_CALLBACK {
            STREAM: self, FAREWELL_CALLBACK: action_to_string(&callback)
        });
        self.0.body.borrow_mut().farewell_callback = Some(callback);
    }

    pub fn unregister_farewell_callback(&self) {
        TRACE!(ATEN_FAREWELLSTREAM_UNREGISTER_FAREWELL_CALLBACK {
            STREAM: self
        });
        self.0.body.borrow_mut().farewell_callback = None;
    }
} // impl Stream
