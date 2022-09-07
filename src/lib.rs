#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

pub mod stream;
pub mod misc;

use std::cell::{Ref, RefCell, RefMut};
use std::collections::{BTreeMap, HashMap, LinkedList};
use std::io::{Error, Result};
use std::option::Option;
use std::os::unix::io::{RawFd, AsRawFd};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::{Instant, Duration};
use r3::{TRACE, TRACE_ENABLED, Traceable, errsym};

pub type UID = r3::UID;

pub struct Action {
    pub uid: UID,
    pub f: Rc<Box<dyn Fn() + 'static>>,
}

impl Action {
    pub fn new<F>(f: F) -> Action where F: Fn() + 'static {
        let uid = UID::new();
        Action {
            uid: uid,
            f: Rc::new(Box::new(f)),
        }
    }

    pub fn noop() -> Action {
        Action {
            uid: UID::new(),
            f: Rc::new(Box::new(move || {})),
        }
    }

    pub fn gut(&mut self) -> Action {
        let uid = self.uid;
        TRACE!(ATEN_ACTION_GUT { ACTION: uid });
        Action {
            uid: UID::new(),
            f: std::mem::replace(&mut self.f, Rc::new(Box::new(move || {
                TRACE!(ATEN_ACTION_GUTTED { ACTION: uid });
            }))),
        }
    }

    pub fn perform(&self) {
        TRACE!(ATEN_ACTION_PERFORM { ACTION: self.uid });
        (self.f)();
    }
} // impl Action

impl ToString for Action {
    fn to_string(&self) -> String {
        format!("{}", self.uid)
    }
} // impl ToString for Action

impl Clone for Action {
    fn clone(&self) -> Action {
        Action {
            uid: self.uid,
            f: Rc::clone(&self.f),
        }
    }
} // impl Clone for Action

impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.uid)
    }
} // impl std::fmt::Debug for Action

#[derive(Debug)]
struct FdBody(RawFd);

// Note: no tracing for FdBody as it may be shared by multiple threads
// Note 2: why does that lead to a deadlock?
impl Drop for FdBody {
    fn drop(&mut self) {
        unsafe { libc::close(self.0) };
    }
} // impl Drop for FdBody

#[derive(Debug)]
pub struct Fd(Arc<FdBody>);

impl Fd {
    pub fn new<R>(fd: R) -> Fd where R: AsRawFd {
        Fd(Arc::new(FdBody(fd.as_raw_fd())))
    }

    pub fn clone(&self) -> Fd {
        Fd(Arc::clone(&self.0))
    }
} // impl Fd

impl AsRawFd for Fd {
    fn as_raw_fd(&self) -> RawFd {
        self.0.0
    }
} // impl AsRawFd for Fd

impl std::fmt::Display for Fd {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.as_raw_fd())
    }
} // impl std::fmt::Display for Fd

#[derive(Debug)]
pub struct Link<Body: ?Sized> {
    pub uid: UID,
    pub body: Rc<RefCell<Body>>,
}

#[derive(Debug)]
pub struct WeakLink<Body: ?Sized> {
    pub uid: UID,
    pub body: Weak<RefCell<Body>>,
}

#[derive(Debug)]
enum TimerKind {
    Pending,
    Scheduled,
    Canceled,
}

#[derive(Debug)]
struct TimerBody {
    disk_ref: WeakDisk,
    expires: Instant,
    uid: UID,
    kind: TimerKind,
    action: Action,
    stack_trace: Option<String>,
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
    poll_fd: Fd,
    immediate: LinkedList<Rc<RefCell<TimerBody>>>,
    timers: BTreeMap<(Instant, UID), Rc<RefCell<TimerBody>>>,
    registrations: HashMap<RawFd, Event>,
    quit: bool,
    wakeup_fd: Option<Fd>,
    recent: Instant,
    rounder_upper: Duration,
}

impl Drop for DiskBody {
    fn drop(&mut self) {
        TRACE!(ATEN_DISK_DROP { DISK: self.uid });
    }
} // impl Drop for DiskBody

pub trait Downgradable<T> {
    fn downgrade(&self) -> T;
}

DECLARE_LINKS!(Disk, WeakDisk, DiskBody, ATEN_DISK_UPPED_MISS, DISK);

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
            poll_fd: Fd::new(poll_fd),
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
        if let Some(fd) = &self.body().wakeup_fd {
            let dummy_byte = &0u8 as *const _ as *const libc::c_void;
            if unsafe { libc::write(fd.as_raw_fd(), dummy_byte, 1) } < 0 {
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
            action: action,
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
            ACTION: &action,
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
            ACTION: &action,
        });
        let (timer, timer_ref) = self.new_timer(
            timer_uid, TimerKind::Scheduled, expires, action);
        self.mut_body().timers.insert((expires, timer_uid), timer_ref);
        timer
    }

    pub fn make_event(&self, action: Action) -> Event {
        let event_uid = UID::new();
        TRACE!(ATEN_DISK_EVENT_CREATE {
            DISK: self, EVENT: event_uid, ACTION: &action,
        });
        let stack_trace =
            if TRACE_ENABLED!(ATEN_DISK_TIMER_BT) {
                Some(r3::stack())
            } else {
                None
            };
        let event_body = EventBody {
            weak_disk: self.downgrade(),
            uid: event_uid,
            state: EventState::Idle,
            action: action,
            stack_trace: stack_trace,
        };
        Event(Link {
            uid: event_uid,
            body: Rc::new(RefCell::new(event_body)),
        })
    }

    pub fn fd(&self) -> Fd {
        self.body().poll_fd.clone()
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
                    TRACE!(ATEN_DISK_POLL_TIMER_EXPIRED {
                        DISK: self, TIMER: first_body.uid,
                        ACTION: &first_body.action,
                    });
                    return NextStep::TimerExpired(
                        first_body.expires, first_body.uid);
                }
                TRACE!(ATEN_DISK_POLL_IMMEDIATE {
                    DISK: self, TIMER: front_body.uid,
                    ACTION: &front_body.action,
                });
                return NextStep::ImmediateAction;
            }
            if first_body.expires <= now {
                TRACE!(ATEN_DISK_POLL_TIMER_EXPIRED {
                    DISK: self, TIMER: first_body.uid,
                    ACTION: &first_body.action,
                });
                return NextStep::TimerExpired(
                    first_body.expires, first_body.uid);
            }
            TRACE!(ATEN_DISK_POLL_SLEEP {
                DISK: self, UNTIL: r3::time(first_body.expires),
            });
            return NextStep::NextTimerExpiry(first_body.expires);
        }
        if let Some(front) = body.immediate.front() {
            let front_body = front.borrow();
            TRACE!(ATEN_DISK_POLL_IMMEDIATE {
                DISK: self, TIMER: front_body.uid,
                ACTION: &front_body.action,
            });
            return NextStep::ImmediateAction;
        }
        TRACE!(ATEN_DISK_POLL_IDLE { DISK: self });
        NextStep::InfiniteWait
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
                                ACTION: &timer_body.action,
                            });
                            continue
                        }
                        let action = timer_body.action.gut();
                        TRACE!(ATEN_DISK_POLL_TIMEOUT {
                            DISK: self, TIMER: timer_body.uid,
                            ACTION: &action,
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
                    unreachable!();
                }
                NextStep::TimerExpired(expires, uid) => {
                    let mut body = self.mut_body();
                    if let Some(rc) = body.timers.remove(&(expires, uid)) {
                        return PoppedTimer::TimerExpired(
                            rc.borrow_mut().action.gut());
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
            &self.fd(), &mut vec![],
            self.milliseconds_remaining(until, None)) {
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
        match epoll_wait(&self.fd(), &mut epoll_events, 0) {
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
                action.perform();
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
                    action.perform();
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
            drain.perform();
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
            unlock.perform();
            let result = epoll_wait(&self.fd(), &mut epoll_events, dur_ms);
            lock.perform();
            match result {
                Err(err) => {
                    TRACE!(ATEN_DISK_LOOP_FAIL {
                        DISK: self, ERR: errsym(&err)
                    });
                    return Err(err);
                }
                Ok(0) => {
                    TRACE!(ATEN_DISK_LOOP_TIMEOUT { DISK: self });
                }
                Ok(count) => {
                    for i in 0..count {
                        let event = self.body().registrations.get(
                            &(epoll_events[i].u64 as RawFd)).map(
                            |event| { event.clone() }
                        );
                        // body unborrowed
                        match event {
                            Some(event) => {
                                TRACE!(ATEN_DISK_LOOP_EXECUTE {
                                    DISK: self, EVENT: event
                                });
                                event.trigger();
                            }
                            None => {
                                TRACE!(ATEN_DISK_LOOP_SPURIOUS { DISK: self });
                            }
                        };
                    }
                }
            }
        }
    }

    pub fn main_loop(&self) -> Result<()> {
        TRACE!(ATEN_DISK_LOOP { DISK: self });
        assert!(self.mut_body().wakeup_fd.is_none());
        self.do_loop(Action::noop(), Action::noop(), Action::noop())?;
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
        self.mut_body().wakeup_fd = Some(Fd::new(write_fd));
        self.register(&Fd::new(pipe_fds[0]), Action::noop())
    }

    fn finish_protected_loop(&self) -> Result<()> {
        TRACE!(ATEN_DISK_PROTECTED_LOOP_FINISH { DISK: self });
        self.mut_body().wakeup_fd = None;
        Ok(())
    }

    pub fn protected_loop(&self, lock: Action, unlock: Action) -> Result<()> {
        assert!(self.body().wakeup_fd.is_none());
        let registration = self.prepare_protected_loop()?;
        self.do_loop(
            lock,
            unlock,
            Action::new(move || { drain(&registration.fd); }))?;
        self.finish_protected_loop()
    }

    fn register_with_flags(&self, fd: &Fd, flags: u32, action: Action)
                           -> Result<Registration> {
        if let Err(err) = nonblock(fd) {
            TRACE!(ATEN_DISK_REGISTER_NONBLOCK_FAIL {
                DISK: self, FD: fd, FLAGS: r3::hex(flags as u64),
                ACTION: &action, ERR: errsym(&err),
            });
            return Err(err);
        }
        let mut epoll_event = libc::epoll_event {
	    events: flags,
            u64: fd.as_raw_fd() as u64,
        };
        let status = unsafe {
            libc::epoll_ctl(
                self.fd().as_raw_fd(), libc::EPOLL_CTL_ADD,
                fd.as_raw_fd(), &mut epoll_event)
        };
        if status < 0 {
            let err = Error::last_os_error();
            TRACE!(ATEN_DISK_REGISTER_FAIL {
                DISK: self, FD: fd, FLAGS: r3::hex(flags as u64),
                ACTION: &action, ERR: errsym(&err),
            });
            return Err(err);
        }
        TRACE!(ATEN_DISK_REGISTER {
            DISK: self, FD: fd, FLAGS: r3::hex(flags as u64), ACTION: &action,
        });
        self.mut_body().registrations.insert(
            fd.as_raw_fd(), self.make_event(action));
        self.wake_up();
        Ok(Registration {
            weak_disk: self.downgrade(),
            fd: fd.clone(),
        })
    }

    pub fn register(&self, fd: &Fd, action: Action) -> Result<Registration> {
        self.register_with_flags(
            fd,
            (libc::EPOLLIN | libc::EPOLLOUT | libc::EPOLLET) as u32,
            action)
    }

    pub fn register_old_school(&self, fd: &Fd, action: Action)
                               -> Result<Registration> {
        self.register_with_flags(
            fd,
            libc::EPOLLIN as u32,
            action)
    }

    fn modify_old_school(&self, fd: &Fd, readable: bool, writable: bool)
                         -> Result<()> {
        if !self.body().registrations.contains_key(&fd.as_raw_fd()) {
            return Err(error::badf())
        }
        let mut epoll_event = libc::epoll_event {
	    events: 0,
            u64: fd.as_raw_fd() as u64,
        };
        if readable {
            epoll_event.events |= libc::EPOLLIN as u32;
        };
        if writable {
            epoll_event.events |= libc::EPOLLOUT as u32;
        };
        let status = unsafe {
            libc::epoll_ctl(
                self.fd().as_raw_fd(), libc::EPOLL_CTL_MOD,
                fd.as_raw_fd(), &mut epoll_event)
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

    fn unregister(&self, fd: &Fd) {
        let result = self.mut_body().registrations.remove(&fd.as_raw_fd());
        assert!(result.is_some());
        let mut epoll_events: Vec<libc::epoll_event> = vec![];
        let status = unsafe {
            libc::epoll_ctl(
                self.fd().as_raw_fd(), libc::EPOLL_CTL_DEL,
                fd.as_raw_fd(), epoll_events.as_mut_ptr())
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

#[derive(Debug)]
pub struct Registration {
    weak_disk: WeakDisk,
    fd: Fd,
}

impl Registration {
    pub fn modify_old_school(&self, readable: bool, writable: bool)
                             -> Option<Result<()>> {
        self.weak_disk.upped(|disk| {
            disk.modify_old_school(&self.fd, readable, writable)
        })
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.weak_disk.upped(|disk| {
            disk.unregister(&self.fd);
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

#[derive(Debug)]
struct EventBody {
    weak_disk: WeakDisk,
    uid: UID,
    state: EventState,
    action: Action,
    stack_trace: Option<String>,
}

impl Drop for EventBody {
    fn drop(&mut self) {
        TRACE!(ATEN_DISK_EVENT_DROP { EVENT: self.uid });
    }
} // impl Drop for EventBody

DECLARE_LINKS!(Event, WeakEvent, EventBody, ATEN_EVENT_UPPED_MISS, EVENT);

impl EventBody {
    fn set_state(&mut self, state: EventState) {
        TRACE!(ATEN_EVENT_SET_STATE {
            EVENT: self.uid, OLD: &self.state, NEW: &state
        });
        self.state = state;
    }

    fn perf(&mut self) {
        TRACE!(ATEN_EVENT_PERF { EVENT: self.uid });
        match self.state {
            EventState::Idle => { unreachable!(); }
            EventState::Triggered => {
                self.set_state(EventState::Idle);
                self.action.perform();
            }
            EventState::Canceled => {
                self.set_state(EventState::Idle);
            }
        };
    }

    fn trigger(&mut self, weak_self: WeakEvent) {
        TRACE!(ATEN_EVENT_TRIGGER { EVENT: self.uid });
        match self.state {
            EventState::Idle => {
                self.set_state(EventState::Triggered);
                self.weak_disk.upped(|disk| {
                    let weak_self = weak_self.clone();
                    disk.execute(Action::new(move || {
                        weak_self.upped(|event| {
                            event.0.body.borrow_mut().perf();
                        });
                    }));
                });
            }
            EventState::Triggered => {}
            EventState::Canceled => {
                self.set_state(EventState::Triggered);
            }
        }
    }

    fn cancel(&mut self) {
        TRACE!(ATEN_EVENT_CANCEL { EVENT: self.uid });
        match self.state {
            EventState::Idle | EventState::Canceled => {}
            EventState::Triggered => {
                self.set_state(EventState::Canceled);
            }
        }
    }
}

impl Event {
    pub fn trigger(&self) {
        self.0.body.borrow_mut().trigger(self.downgrade());
    }

    pub fn cancel(&self) {
        self.0.body.borrow_mut().cancel();
    }
} // impl Event

pub fn nonblock(fd: &Fd) -> Result<()> {
    let status = unsafe {
        libc::fcntl(fd.as_raw_fd(), libc::F_GETFL, 0)
    };
    if status < 0 {
        return Err(Error::last_os_error());
    }
    let status = unsafe {
        libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, status | libc::O_NONBLOCK)
    };
    if status < 0 {
        return Err(Error::last_os_error());
    }
    Ok(())
}

fn drain(fd: &Fd) {
    let mut buffer = vec![0u8; 1024];
    loop {
        let count = unsafe {
            libc::read(fd.as_raw_fd(),
                       buffer.as_mut_ptr() as *mut libc::c_void,
                       buffer.len())
        };
        if count <= 0 {
            break
        }
    }
}

fn epoll_wait(fd: &Fd, epoll_events: &mut Vec<libc::epoll_event>,
              ms: libc::c_int) -> Result<usize> {
    let count = unsafe {
        libc::epoll_wait(fd.as_raw_fd(), epoll_events.as_mut_ptr(),
                         epoll_events.len() as libc::c_int, ms)
    };
    if count < 0 {
        Err(Error::last_os_error())
    } else {
        Ok(count as usize)
    }
}

pub mod error;

#[macro_export]
macro_rules! DISPLAY_LINK_UID {
    ($Type:ident) => {
        impl std::fmt::Display for $Type {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.0.uid)
            }
        }
    }
}

#[macro_export]
macro_rules! DECLARE_LINKS {
    ($Strong:ident,
     $Weak:ident,
     $Body:ty,
     $UPPED_MISS:ident,
     $SELF:ident) => {
        #[derive(Debug)]
        pub struct $Strong($crate::Link<$Body>);

        impl $crate::Downgradable<$Weak> for $Strong {
            fn downgrade(&self) -> $Weak {
                $Weak($crate::WeakLink {
                    uid: self.0.uid,
                    body: std::rc::Rc::downgrade(&self.0.body),
                })
            }
        }

        impl std::clone::Clone for $Strong {
            fn clone(&self) -> Self {
                Self($crate::Link {
                    uid: self.0.uid,
                    body: self.0.body.clone(),
                })
            }
        }

        impl std::fmt::Display for $Strong {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.0.uid)
            }
        }

        pub struct $Weak($crate::WeakLink<$Body>);

        impl $Weak {
            pub fn upgrade(&self) -> Option<$Strong> {
                self.0.body.upgrade().map(|body|
                                          $Strong($crate::Link {
                                              uid: self.0.uid,
                                              body: body,
                                          }))
            }

            pub fn upped<F, R>(&self, f: F) -> Option<R>
            where F: Fn(&$Strong) -> R {
                match self.upgrade() {
                    Some(thing) => Some(f(&thing)),
                    None => {
                        r3::TRACE!($UPPED_MISS {
                            $SELF: self as &dyn r3::Traceable
                        });
                        None
                    }
                }
            }
        }

        impl std::clone::Clone for $Weak {
            fn clone(&self) -> Self {
                Self($crate::WeakLink {
                    uid: self.0.uid,
                    body: self.0.body.clone(),
                })
            }
        }

        impl std::fmt::Display for $Weak {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.0.uid)
            }
        }

        impl std::fmt::Debug for $Weak {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($Weak), self.0.uid)
            }
        }
    }
}
