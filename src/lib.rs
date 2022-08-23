#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

pub mod stream;
pub mod misc;

use std::cell::{Ref, RefCell, RefMut};
use std::collections::{BTreeMap, HashMap, LinkedList};
use std::io::{Error, Result};
use std::option::Option;
use std::os::unix::io::RawFd;
use std::rc::{Rc, Weak};
use std::time::{Instant, Duration};
use r3::{TRACE, TRACE_ENABLED, Traceable, errsym};

pub type UID = r3::UID;
pub type Action = Rc<dyn Fn() + 'static>;

pub fn format_action(f: &mut std::fmt::Formatter, action: &Action)
                     -> std::fmt::Result {
    write!(f, "{:?}", Rc::as_ptr(action))
}

pub fn action_to_string(action: &Action) -> String {
    format!("{:?}", Rc::as_ptr(action))
}

pub fn format_callback(f: &mut std::fmt::Formatter, callback: &Option<Action>)
                       -> std::fmt::Result {
    if let Some(action) = callback {
        format_action(f, &action)
    } else {
        Ok(())
    }
}

pub fn callback_to_string(callback: &Option<Action>) -> String {
    if let Some(action) = callback {
        action_to_string(action)
    } else {
        "".to_string()
    }
}

#[derive(Debug)]
struct Link<Body: ?Sized> {
    uid: UID,
    body: Rc<RefCell<Body>>,
}

#[derive(Debug)]
struct WeakLink<Body: ?Sized> {
    uid: UID,
    body: Weak<RefCell<Body>>,
}

#[derive(Debug)]
enum TimerKind {
    Pending,
    Scheduled,
    Canceled,
}

struct TimerBody {
    disk_ref: WeakDisk,
    expires: Instant,
    uid: UID,
    kind: TimerKind,
    action: Option<Action>,
    stack_trace: Option<String>,
}

impl std::fmt::Debug for TimerBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimerBody")
            .field("expires", &self.expires)
            .field("uid", &self.uid)
            .field("kind", &self.kind)
            .field("action", &callback_to_string(&self.action))
            .field("stack_trace", &self.stack_trace)
            .finish()
    }
}

#[derive(Debug)]
pub struct Timer(WeakLink<TimerBody>);

impl Timer {
    pub fn cancel(&self) {
        TRACE!(ATEN_TIMER_CANCEL { TIMER: self });
        if let Some(cell) = self.0.body.upgrade() {
            let mut body = cell.borrow_mut();
            if let TimerKind::Scheduled = body.kind {
                if let Some(disk_ref) = body.disk_ref.upgrade() {
                    disk_ref.mut_body().timers.remove(
                        &(body.expires, body.uid));
                }
            }
            body.kind = TimerKind::Canceled
        }
    }

    pub fn downgrade(&self) -> WeakTimer {
        self.clone()
    }

    pub fn upgrade(&self) -> Option<Timer> {
        Some(self.clone())
    }

    pub fn upped<F, R>(&self, f: F) -> Option<R> where F: Fn(&Timer) -> R {
        match self.upgrade() {
            Some(timer) => Some(f(&timer)),
            None => {
                TRACE!(ATEN_TIMER_UPPED_MISS { TIMER: self });
                None
            }
        }
    }
} // impl Timer

impl Clone for Timer {
    fn clone(&self) -> Timer {
        Timer(WeakLink {
            uid: self.0.uid,
            body: self.0.body.clone(),
        })
    }
} // impl Clone for Timer

DISPLAY_LINK_UID!(Timer);

type WeakTimer = Timer;

#[derive(Debug)]
struct DiskBody {
    uid: UID,
    poll_fd: RawFd,
    immediate: LinkedList<Rc<RefCell<TimerBody>>>,
    timers: BTreeMap<(Instant, UID), Rc<RefCell<TimerBody>>>,
    registrations: HashMap<RawFd, Event>,
    quit: bool,
    wakeup_fd: Option<RawFd>,
    recent: Instant,
    rounder_upper: Duration,
}

impl Drop for DiskBody {
    fn drop(&mut self) {
        TRACE!(ATEN_DISK_DROP { DISK: self.uid });
        unsafe { libc::close(self.poll_fd) };
        if let Some(fd) = self.wakeup_fd {
            unsafe { libc::close(fd) };
        }
    }
} // impl Drop for DiskBody

#[derive(Debug)]
pub struct Disk(Link<DiskBody>);

impl Disk {
    pub fn new() -> Result<Disk> {
        let poll_fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if poll_fd < 0 {
            let err = Error::last_os_error();
            TRACE!(ATEN_DISK_EPOLL_CREATE_FAILED { ERR: errsym(&err) });
            return Err(err);
        }
        let uid = UID::new();
        let body = DiskBody {
            uid: uid,
            poll_fd: poll_fd,
            immediate: LinkedList::new(),
            timers: BTreeMap::new(),
            registrations: HashMap::new(),
            quit: false,
            wakeup_fd: None,
            recent: Instant::now(),
            rounder_upper: Duration::from_millis(1) - Duration::from_nanos(1),
        };
        let disk = Disk(Link {
            uid: uid,
            body: Rc::new(RefCell::new(body)),
        });
        disk.now();
        TRACE!(ATEN_DISK_CREATE { DISK: uid, POLL_FD: poll_fd });
        Ok(disk)
    }

    fn body(&self) -> Ref<DiskBody> {
        self.0.body.borrow()
    }

    fn mut_body(&self) -> RefMut<DiskBody> {
        self.0.body.borrow_mut()
    }

    pub fn now(&self) -> Instant {
        let t = Instant::now();
        self.mut_body().recent = t;
        t
    }

    pub fn wake_up(&self) {
        TRACE!(ATEN_DISK_WAKE_UP { DISK: self });
        if let Some(fd) = self.body().wakeup_fd {
            let dummy_byte = &0u8 as *const _ as *const libc::c_void;
            if unsafe { libc::write(fd, dummy_byte, 1) } < 0 {
                assert_eq!(Error::last_os_error().raw_os_error(),
                           Some(libc::EAGAIN));
            }
        }
    }

    fn new_timer(&self, uid: UID, kind: TimerKind, expires: Instant,
                 action: Action)
                 -> (Timer, Rc<RefCell<TimerBody>>) {
        self.wake_up();
        let stack_trace =
            if TRACE_ENABLED!(ATEN_DISK_TIMER_BT) {
                Some(r3::stack())
            } else {
                None
            };
        let timer_ref = Rc::new(RefCell::new(TimerBody {
            disk_ref: self.downgrade(),
            expires: expires,
            uid: uid,
            kind: kind,
            action: Some(action),
            stack_trace: stack_trace,
        }));
        let uid = timer_ref.borrow().uid;
        let timer = Timer(WeakLink {
            uid: uid,
            body: Rc::downgrade(&timer_ref),
        });
        (timer, timer_ref)
    }

    pub fn execute(&self, action: Action) -> Timer {
        let now = self.body().recent;
        let timer_uid = UID::new();
        TRACE!(ATEN_DISK_EXECUTE {
            DISK: self, TIMER: timer_uid, EXPIRES: r3::time(now),
            ACTION: action_to_string(&action),
        });
        let (timer, timer_ref) = self.new_timer(
            timer_uid, TimerKind::Pending, now, action);
        self.mut_body().immediate.push_back(timer_ref);
        timer
    }

    pub fn schedule(&self, expires: Instant, action: Action) -> Timer {
        let timer_uid = UID::new();
        TRACE!(ATEN_DISK_SCHEDULE {
            DISK: self, TIMER: timer_uid, EXPIRES: r3::time(expires),
            ACTION: action_to_string(&action),
        });
        let (timer, timer_ref) = self.new_timer(
            timer_uid, TimerKind::Scheduled, expires, action);
        self.mut_body().timers.insert((expires, timer_uid), timer_ref);
        timer
    }

    pub fn make_event(&self, action: Action) -> Event {
        let event_uid = UID::new();
        TRACE!(ATEN_DISK_EVENT_CREATE {
            DISK: self, EVENT: event_uid, ACTION: action_to_string(&action),
        });
        let stack_trace =
            if TRACE_ENABLED!(ATEN_DISK_TIMER_BT) {
                Some(r3::stack())
            } else {
                None
            };
        let event_body = EventBody {
            disk_ref: self.downgrade(),
            uid: event_uid,
            state: EventState::Idle,
            action: Some(action),
            stack_trace: stack_trace,
        };
        Event(Link {
            uid: event_uid,
            body: Rc::new(RefCell::new(event_body)),
        })
    }

    pub fn fd(&self) -> RawFd {
        self.body().poll_fd
    }

    fn next_step(&self) -> NextStep {
        let now = self.now();
        let body = self.body();
        for (_, first) in body.timers.iter() {
            let first_body = first.borrow();
            if let Some(front) = body.immediate.front() {
                let first_key = (first_body.expires, first_body.uid);
                let front_body = front.borrow();
                let front_key = (front_body.expires, front_body.uid);
                if first_key < front_key {
                    return NextStep::TimerExpired(
                        first_body.expires, first_body.uid);
                }
                return NextStep::ImmediateAction;
            }
            if first_body.expires <= now {
                return NextStep::TimerExpired(
                    first_body.expires, first_body.uid);
            }
            return NextStep::NextTimerExpiry(first_body.expires);
        }
        if body.immediate.is_empty() {
            NextStep::InfiniteWait
        } else {
            NextStep::ImmediateAction
        }
    }

    fn pop_timer(&self) -> PoppedTimer {
        loop {
            match self.next_step() {
                NextStep::ImmediateAction => {
                    let mut body = self.mut_body();
                    if let Some(rc) = body.immediate.pop_front() {
                        let mut timer_body = rc.borrow_mut();
                        if let TimerKind::Canceled = timer_body.kind {
                            TRACE!(ATEN_DISK_POLL_TIMER_CANCELED {
                                DISK: self, TIMER: timer_body.uid,
                            });
                            continue
                        }
                        if let Some(action) = timer_body.action.take() {
                            TRACE!(ATEN_DISK_POLL_TIMEOUT {
                                DISK: self, TIMER: timer_body.uid,
                                ACTION: action_to_string(&action),
                            });
                            if TRACE_ENABLED!(ATEN_DISK_TIMER_BT) {
                                if let Some(stack) = &timer_body.stack_trace {
                                    TRACE!(ATEN_DISK_TIMER_BT {
                                        DISK: self, TIMER: timer_body.uid,
                                        BT: stack,
                                    })
                                }
                            }
                            return PoppedTimer::TimerExpired(action);
                        }
                    }
                    unreachable!();
                }
                NextStep::TimerExpired(expires, uid) => {
                    let mut body = self.mut_body();
                    if let Some(rc) = body.timers.remove(&(expires, uid)) {
                        if let Some(action) = rc.borrow_mut().action.take() {
                            return PoppedTimer::TimerExpired(action);
                        }
                    }
                    unreachable!();
                }
                NextStep::NextTimerExpiry(expiry) => {
                    TRACE!(ATEN_DISK_POLL_NEXT_TIMER {
                        DISK: self, EXPIRY: r3::time(expiry)
                    });
                    return PoppedTimer::NextTimerExpiry(expiry);
                }
                NextStep::InfiniteWait => {
                    TRACE!(ATEN_DISK_POLL_NO_TIMERS { DISK: self });
                    return PoppedTimer::InfiniteWait;
                }
            }
        }
    }

    fn milliseconds_remaining(&self, until: Instant, cap: Option<Duration>)
                              -> i32 {
        let mut duration = until.saturating_duration_since(self.now());
        if let Some(max_duration) = cap {
            if max_duration < duration {
                duration = max_duration;
            }
        }
        let ms = (duration + self.body().rounder_upper).as_millis();
        if ms > i32::MAX as u128 {
            i32::MAX
        } else {
            ms as i32
        }
    }

    fn sleep(&self, until: Instant) -> Result<()> {
        if let Err(err) = epoll_wait(
            self.fd(), &mut vec![], self.milliseconds_remaining(until, None)) {
            TRACE!(ATEN_DISK_SLEEP_FAIL { DISK: self, ERR: errsym(&err) });
            return Err(err);
        }
        Ok(())
    }

    fn try_io(&self, next_expiry: Instant) -> Result<Option<Instant>> {
        let body = self.body();
        let mut epoll_events = vec![libc::epoll_event {
            events: 0,
            u64: 0,
        }];
        match epoll_wait(self.fd(), &mut epoll_events, 0) {
            Err(err) => {
                TRACE!(ATEN_DISK_POLL_FAIL { DISK: self, ERR: errsym(&err) });
                return Err(err);
            }
            Ok(0) => {
                TRACE!(ATEN_DISK_POLL_SPURIOUS { DISK: self });
                return Ok(Some(next_expiry));
            }
            Ok(_) => {}
        }
        match body.registrations.get(&(epoll_events[0].u64 as RawFd)) {
            Some(event) => {
                TRACE!(ATEN_DISK_POLL_EXECUTE { DISK: self, EVENT: &event });
                event.trigger();
            }
            None => {
                TRACE!(ATEN_DISK_POLL_EXECUTE_SPURIOUS { DISK: self });
            }
        }
        return Ok(Some(body.recent));
    }

    pub fn poll(&self) -> Result<Option<Instant>> {
        match self.pop_timer() {
            PoppedTimer::TimerExpired(action) => {
                (action)();
                return Ok(Some(self.body().recent));
            }
            PoppedTimer::NextTimerExpiry(expiry) => {
                return self.try_io(expiry);
            }
            PoppedTimer::InfiniteWait => {
                return Ok(None);
            }
        }
    }

    pub fn quit(&self) {
        TRACE!(ATEN_DISK_QUIT { DISK: self });
        self.mut_body().quit = true;
        self.wake_up();
    }

    fn take_immediate_action(&self) -> Option<Instant> {
        const MAX_IO_STARVATION: u8 = 20;
        let mut countdown = MAX_IO_STARVATION;
        while countdown > 0 {
            match self.pop_timer() {
                PoppedTimer::TimerExpired(action) => {
                    (action)();
                    countdown -= 1;
                }
                PoppedTimer::NextTimerExpiry(expiry) => {
                    return Some(expiry);
                }
                PoppedTimer::InfiniteWait => {
                    return None;
                }
            }
        }
        Some(self.mut_body().recent)
    }

    fn do_loop(&self, lock: Action, unlock: Action, drain: Action)
               -> Result<()> {
        const MAX_IO_BURST: u8 = 20;
        loop {
            (drain)();
            let result = self.take_immediate_action();
            if self.body().quit {
                TRACE!(ATEN_DISK_LOOP_QUIT { DISK: self });
                return Ok(());
            }
            let dur_ms =
                if let Some(expiry) = result {
                    self.milliseconds_remaining(expiry, None)
                } else {
                    -1
                };
            TRACE!(ATEN_DISK_LOOP_WAIT { DISK: self, DUR_MS: dur_ms });
            let mut epoll_events = vec![libc::epoll_event {
                events: 0,
                u64: 0,
            }; MAX_IO_BURST as usize];
            (unlock)();
            let result = epoll_wait(self.fd(), &mut epoll_events, dur_ms);
            (lock)();
            match result {
                Err(err) => {
                    TRACE!(ATEN_DISK_LOOP_FAIL {
                        DISK: self, ERR: errsym(&err)
                    });
                    return Err(err);
                }
                Ok(count) => {
                    for i in 0..count {
                        match self.body().registrations.get(
                            &(epoll_events[i].u64 as RawFd)) {
                            Some(event) => {
                                TRACE!(ATEN_DISK_LOOP_EXECUTE {
                                    DISK: self, EVENT: event
                                });
                                event.trigger();
                            }
                            None => {
                                TRACE!(ATEN_DISK_LOOP_SPURIOUS { DISK: self });
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn main_loop(&self) -> Result<()> {
        TRACE!(ATEN_DISK_LOOP { DISK: self });
        assert!(self.mut_body().wakeup_fd.is_none());
        let noop = Rc::new(move || {});
        self.do_loop(noop.clone(), noop.clone(), noop)?;
        TRACE!(ATEN_DISK_LOOP_FINISH { DISK: self });
        Ok(())
    }

    fn prepare_protected_loop(&self) -> Result<Registration> {
        let mut pipe_fds = vec![0 as libc::c_int; 2];
        let status = unsafe {
            libc::pipe2(pipe_fds.as_mut_ptr(), libc::O_CLOEXEC)
        };
        if status < 0 {
            let err = Error::last_os_error();
            TRACE!(ATEN_DISK_PROTECTED_LOOP_FAIL {
                DISK: self, ERR: errsym(&err)
            });
            return Err(err);
        }
        TRACE!(ATEN_DISK_PROTECTED_LOOP { DISK: self });
        let write_fd = pipe_fds[1] as RawFd;
        self.mut_body().wakeup_fd = Some(write_fd);
        self.register(pipe_fds[0] as RawFd, Rc::new(|| {}))
    }

    fn finish_protected_loop(&self, registration: Registration) -> Result<()> {
        TRACE!(ATEN_DISK_PROTECTED_LOOP_FINISH { DISK: self });
        let mut body = self.mut_body();
        let wakup_fd =
            if let Some(fd) = body.wakeup_fd {
                fd
            } else {
                panic!("wakeup_fd unset")
            };
        unsafe { libc::close(registration.fd) };
        unsafe { libc::close(wakup_fd) };
        body.wakeup_fd = None;
        Ok(())
    }

    pub fn protected_loop(&self, lock: Action, unlock: Action) -> Result<()> {
        assert!(self.body().wakeup_fd.is_none());
        let registration = self.prepare_protected_loop()?;
        self.do_loop(
            lock,
            unlock,
            Rc::new(move || { drain(registration.fd); }))?;
        self.finish_protected_loop(registration)
    }

    fn register_with_flags(&self, fd: RawFd, flags: u32, action: Action)
                           -> Result<Registration> {
        if let Err(err) = nonblock(fd) {
            TRACE!(ATEN_DISK_REGISTER_NONBLOCK_FAIL {
                DISK: self, FD: fd, FLAGS: r3::hex(flags as u64),
                ACTION: action_to_string(&action), ERR: errsym(&err),
            });
            return Err(err);
        }
        let mut epoll_event = libc::epoll_event {
	    events: flags,
            u64: fd as u64,
        };
        let status = unsafe {
            libc::epoll_ctl(self.fd(), libc::EPOLL_CTL_ADD, fd, &mut epoll_event)
        };
        if status < 0 {
            let err = Error::last_os_error();
            TRACE!(ATEN_DISK_REGISTER_FAIL {
                DISK: self, FD: fd, FLAGS: r3::hex(flags as u64),
                ACTION: action_to_string(&action), ERR: errsym(&err),
            });
            return Err(err);
        }
        TRACE!(ATEN_DISK_REGISTER {
            DISK: self, FD: fd, FLAGS: r3::hex(flags as u64),
            ACTION: action_to_string(&action)
        });
        self.mut_body().registrations.insert(
            fd, self.make_event(action));
        self.wake_up();
        Ok(Registration {
            weak_disk: self.downgrade(),
            fd: fd,
        })
    }

    pub fn register(&self, fd: RawFd, action: Action) -> Result<Registration> {
        self.register_with_flags(
            fd,
            (libc::EPOLLIN | libc::EPOLLOUT | libc::EPOLLET) as u32,
            action)
    }

    pub fn register_old_school(&self, fd: RawFd, action: Action)
                               -> Result<Registration> {
        self.register_with_flags(
            fd,
            libc::EPOLLIN as u32,
            action)
    }

    fn modify_old_school(&self, fd: RawFd, readable: bool, writable: bool)
                         -> Result<()> {
        if !self.body().registrations.contains_key(&fd) {
            return Err(Error::from_raw_os_error(libc::EBADF))
        }
        let mut epoll_event = libc::epoll_event {
	    events: 0,
            u64: fd as u64,
        };
        if readable {
            epoll_event.events |= libc::EPOLLIN as u32;
        };
        if writable {
            epoll_event.events |= libc::EPOLLOUT as u32;
        };
        let status = unsafe {
            libc::epoll_ctl(self.fd(), libc::EPOLL_CTL_MOD, fd, &mut epoll_event)
        };
        if status < 0 {
            let err = Error::last_os_error();
            TRACE!(ATEN_DISK_MODIFY_OLD_SCHOOL_FAIL {
                DISK: self, FD: fd, READABLE: readable, WRITABLE: writable,
                ERR: errsym(&err)
            });
            return Err(err);
        }
        TRACE!(ATEN_DISK_MODIFY_OLD_SCHOOL {
            DISK: self, FD: fd, READABLE: readable, WRITABLE: writable,
        });
        self.wake_up();
        Ok(())
    }

    fn unregister(&self, fd: RawFd) {
        if let Some(_) = self.mut_body().registrations.remove(&fd) {
        } else {
            panic!("unregistering file descriptor that has not been registered");
        }
        let mut epoll_events: Vec<libc::epoll_event> = vec![];
        let status = unsafe {
            libc::epoll_ctl(
                self.fd(), libc::EPOLL_CTL_DEL, fd, epoll_events.as_mut_ptr())
        };
        if status < 0 {
            let err = Error::last_os_error();
            TRACE!(ATEN_DISK_UNREGISTER_FAIL {
                DISK: self, FD: fd, ERR: errsym(&err)
            });
            panic!("unregistration failed {:?}", err);
        }
        TRACE!(ATEN_DISK_UNREGISTER { DISK: self, FD: fd });
    }

    pub fn flush(&self, expires: Instant) -> Result<()> {
        TRACE!(ATEN_DISK_FLUSH { DISK: self, EXPIRES: r3::time(expires) });
        loop {
            let now = self.now();
            if now >= expires {
                TRACE!(ATEN_DISK_FLUSH_EXPIRED { DISK: self });
                return Err(Error::from_raw_os_error(libc::ETIME));
            }
            match self.poll() {
                Ok(pop) => {
                    if let Some(expiry) = pop {
                        self.sleep(expiry)?;
                    } else {
                        return Ok(());
                    }
                }
                Err(err) => {
                    TRACE!(ATEN_DISK_FLUSH_FAIL {
                        DISK: self, ERR: errsym(&err)
                    });
                    return Err(err);
                }
            }
        }
    }

    pub fn downgrade(&self) -> WeakDisk {
        WeakDisk(WeakLink {
            uid: self.0.uid,
            body: Rc::downgrade(&self.0.body),
        })
    }

    pub fn in_secs(&self, n: u64) -> Instant {
        self.now() + Duration::from_secs(n)
    }

    pub fn in_millis(&self, n: u64) -> Instant {
        self.now() + Duration::from_millis(n)
    }

    pub fn in_micross(&self, n: u64) -> Instant {
        self.now() + Duration::from_micros(n)
    }

    pub fn in_nanos(&self, n: u64) -> Instant {
        self.now() + Duration::from_nanos(n)
    }

    pub fn in_secs_f32(&self, x: f32) -> Instant {
        self.now() + Duration::from_secs_f32(x)
    }

    pub fn in_secs_f64(&self, x: f64) -> Instant {
        self.now() + Duration::from_secs_f64(x)
    }
} // impl Disk

DISPLAY_LINK_UID!(Disk);

#[derive(Debug)]
pub struct WeakDisk(WeakLink<DiskBody>);

impl WeakDisk {
    pub fn upgrade(&self) -> Option<Disk> {
        self.0.body.upgrade().map(|body|
            Disk(Link {
                uid: self.0.uid,
                body: body,
            }))
    }

    pub fn upped<F, R>(&self, f: F) -> Option<R> where F: Fn(&Disk) -> R {
        match self.upgrade() {
            Some(disk) => Some(f(&disk)),
            None => {
                TRACE!(ATEN_DISK_UPPED_MISS { DISK: self.0.uid });
                None
            }
        }
    }
} // impl WeakDisk

#[derive(Debug)]
pub struct Registration {
    weak_disk: WeakDisk,
    fd: RawFd,
}

impl Registration {
    pub fn modify_old_school(&self, readable: bool, writable: bool)
                             -> Option<Result<()>> {
        self.weak_disk.upped(|disk| {
            disk.modify_old_school(self.fd, readable, writable)
        })
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.weak_disk.upped(|disk| {
            disk.unregister(self.fd);
        }).or_else(|| {
            TRACE!(ATEN_DISK_REGISTRATION_DROP_POSTHUMOUSLY {
                DISK: self.weak_disk.0.uid, FD: self.fd
            });
            None
        });
    }
}

#[derive(Debug)]
enum NextStep {
    ImmediateAction,
    TimerExpired(Instant, UID),
    NextTimerExpiry(Instant),
    InfiniteWait,
}

enum PoppedTimer {
    TimerExpired(Action),
    NextTimerExpiry(Instant),
    InfiniteWait,
}

impl std::fmt::Debug for PoppedTimer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Self::TimerExpired(_) => write!(f, "TimerExpired"),
            Self::NextTimerExpiry(t) => write!(f, "NextTimerExpiry({:?})", t),
            Self::InfiniteWait => write!(f, "InfiniteWait"),
        }
    }
}

#[derive(Debug)]
enum EventState {
    Idle,
    Triggered,
    Canceled,
}

impl std::fmt::Display for EventState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
} // impl std::fmt::Display for EventState

struct EventBody {
    disk_ref: WeakDisk,
    uid: UID,
    state: EventState,
    action: Option<Action>,
    stack_trace: Option<String>,
}

impl std::fmt::Debug for EventBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBody")
            .field("uid", &self.uid)
            .field("state", &self.state)
            .field("action", &callback_to_string(&self.action))
            .field("stack_trace", &self.stack_trace)
            .finish()
    }
}

impl Drop for EventBody {
    fn drop(&mut self) {
        TRACE!(ATEN_DISK_EVENT_DROP { EVENT: self.uid });
    }
} // impl Drop for EventBody

#[derive(Debug)]
pub struct Event(Link<EventBody>);

impl Event {
    fn set_state(&self, state: EventState) {
        let mut body = self.0.body.borrow_mut();
        TRACE!(ATEN_EVENT_SET_STATE {
            EVENT: self, OLD: body.state, NEW: state
        });
        body.state = state;
    }

    fn perf(&self) {
        TRACE!(ATEN_EVENT_PERF { EVENT: self });
        match self.0.body.borrow().state {
            EventState::Idle => { unreachable!(); }
            EventState::Triggered => {
                self.set_state(EventState::Idle);
                if let Some(action) = self.0.body.borrow_mut().action.take() {
                    (action)();
                } else {
                    unreachable!();
                }
            }
            EventState::Canceled => {
                self.set_state(EventState::Idle);
            }
        };
    }

    pub fn trigger(&self) {
        TRACE!(ATEN_EVENT_TRIGGER { EVENT: self });
        match self.0.body.borrow().state {
            EventState::Idle => {
                self.set_state(EventState::Triggered);
                match self.0.body.borrow_mut().disk_ref.upgrade() {
                    Some(disk_ref) => {
                        let self_ref = self.downgrade();
                        let delegate = move || {
                            if let Some(event) = self_ref.upgrade() {
                                event.perf();
                            }
                        };
                        disk_ref.execute(Rc::new(delegate));
                    }
                    None => {
                        return;
                    }
                }
            }
            EventState::Triggered => {}
            EventState::Canceled => {
                self.set_state(EventState::Triggered);
            }
        }
    }

    pub fn cancel(&self) {
        TRACE!(ATEN_EVENT_CANCEL { EVENT: self });
        match self.0.body.borrow().state {
            EventState::Idle | EventState::Canceled => {}
            EventState::Triggered => {
                self.set_state(EventState::Canceled);
            }
        }
    }

    pub fn downgrade(&self) -> WeakEvent {
        WeakEvent(WeakLink {
            uid: self.0.uid,
            body: Rc::downgrade(&self.0.body),
        })
    }
} // impl Event

DISPLAY_LINK_UID!(Event);

#[derive(Debug)]
pub struct WeakEvent(WeakLink<EventBody>);

impl WeakEvent {
    pub fn upgrade(&self) -> Option<Event> {
        self.0.body.upgrade().map(|body|
            Event(Link {
                uid: self.0.uid,
                body: body,
            })
        )
    }

    pub fn upped<F, R>(&self, f: F) -> Option<R> where F: Fn(&Event) -> R {
        match self.upgrade() {
            Some(event) => Some(f(&event)),
            None => {
                TRACE!(ATEN_EVENT_UPPED_MISS { EVENT: self.0.uid });
                None
            }
        }
    }
} // impl WeakEvent
fn nonblock(fd: RawFd) -> Result<()> {
    let status = unsafe {
        libc::fcntl(fd, libc::F_GETFL, 0)
    };
    if status < 0 {
        return Err(Error::last_os_error());
    }
    let status = unsafe {
        libc::fcntl(fd, libc::F_SETFL, status | libc::O_NONBLOCK)
    };
    if status < 0 {
        return Err(Error::last_os_error());
    }
    Ok(())
}

fn drain(fd: RawFd) {
    const BUF_SIZE: usize = 1024;
    let mut buffer = vec![0u8; BUF_SIZE];
    loop {
        let count = unsafe {
            libc::read(fd, buffer.as_mut_ptr() as *mut libc::c_void, BUF_SIZE)
        };
        if count <= 0 {
            break
        }
    }
}

fn epoll_wait(fd: RawFd, epoll_events: &mut Vec<libc::epoll_event>,
              ms: libc::c_int) -> Result<usize> {
    let count = unsafe {
        libc::epoll_wait(fd, epoll_events.as_mut_ptr(),
                         epoll_events.len() as libc::c_int, ms)
    };
    if count < 0 {
        Err(Error::last_os_error())
    } else {
        Ok(count as usize)
    }
}

pub fn again() -> Error {
    Error::from_raw_os_error(libc::EAGAIN)
}

pub fn is_again(err: &Error) -> bool {
    if let Some(errno) = err.raw_os_error() {
        errno == libc::EAGAIN
    } else {
        false
    }
}

#[macro_export]
macro_rules! DISPLAY_LINK_UID {
    ($typename:ident) => {
        impl std::fmt::Display for $typename {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.0.uid)
            }
        } // impl std::fmt::Display for DryStreamBody
    }
}
