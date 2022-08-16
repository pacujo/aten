#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

pub mod stream;

use std::cell::{Ref, RefCell, RefMut};
use std::collections::{BTreeMap, HashMap, LinkedList};
use std::io::{Error, Result};
use std::option::Option;
use std::os::unix::io::RawFd;
use std::rc::{Rc, Weak};
use r3::TRACE;

pub type UID = r3::UID;
pub type Action = Rc<dyn Fn()>;

pub fn format_action(f: &mut std::fmt::Formatter, action: &Action)
                     -> std::fmt::Result {
    write!(f, "{:?}", Rc::as_ptr(action))
}

pub fn action_to_string(action: &Action) -> String {
    format!("{:?}", Rc::as_ptr(action))
}

#[derive(Debug, Copy, Clone)]
pub struct Time(u64);

impl Time {
    pub fn immemorial() -> Time {
        Time(0)
    }

    fn now() -> Time {
        let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0, };
        let _x = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut t) };
        let ns = t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64;
        Time(ns)
    }
} // impl Time

impl PartialEq for Time {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
} // impl PartialEq for Time

impl Eq for Time {}

impl PartialOrd for Time {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }

    fn lt(&self, other: &Self) -> bool {
        self.0.lt(&other.0)
    }

    fn le(&self, other: &Self) -> bool {
        self.0.le(&other.0)
    }

    fn gt(&self, other: &Self) -> bool {
        self.0.gt(&other.0)
    }

    fn ge(&self, other: &Self) -> bool {
        self.0.ge(&other.0)
    }
}

impl Ord for Time {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }

    fn max(self, other: Self) -> Self {
        Time(self.0.max(other.0))
    }

    fn min(self, other: Self) -> Self {
        Time(self.0.min(other.0))
    }

    fn clamp(self, min: Self, max: Self) -> Self {
        Time(self.0.clamp(min.0, max.0))
    }
} // impl Ord for Time

impl std::ops::Add<Duration> for Time {
    type Output = Time;

    fn add(self, other: Duration) -> Time {
        let sum = self.0.wrapping_add(other.0 as u64);
        assert!(if other.0 < 0 { sum < self.0 } else { sum >= self.0 });
        Time(sum)
    }
} // impl std::ops::Add for Time

impl std::ops::Sub for Time {
    type Output = Duration;

    fn sub(self, other: Self) -> Duration {
        let diff = self.0.wrapping_sub(other.0) as i64;
        assert!(if self.0 >= other.0 { diff >= 0 } else { diff < 0 });
        Duration(diff)
    }
} // impl std::ops::Sub for Time

impl std::fmt::Display for Time {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        r3::format_in_threes(f, self.0)
    }
} // impl std::fmt::Display for Time

#[derive(Debug, Copy, Clone)]
pub struct Duration(i64);

impl Duration {
    pub fn from_ns(n: i64) -> Duration { n * Duration(1) }
    pub fn from_μs(n: i64) -> Duration { n * Duration(1_000) }
    pub fn from_us(n: i64) -> Duration { Duration::from_μs(n) }
    pub fn from_ms(n: i64) -> Duration { n * Duration(1_000_000) }
    pub fn from_s(n: i64) -> Duration { n * Duration(1_000_000_000) }
    pub fn from_min(n: i64) -> Duration { n * Duration(60_000_000_000) }
    pub fn from_h(n: i64) -> Duration { n * Duration(3_600_000_000_000) }
    pub fn from_days(n: i64) -> Duration { n * Duration(86_400_000_000_000) }

    pub fn from_f64(x: f64) -> Duration { // given in seconds
        let seconds = x.trunc() as i64;
        assert_ne!(seconds, i64::MAX);
        assert_ne!(seconds, i64::MIN);
        let nanoseconds = (x.fract() * 1e9).round() as i64;
        Duration::from_s(seconds) + Duration::from_ns(nanoseconds)
    }

    pub fn elapsed(&self) -> bool {
        self.0 <= 0
    }

    fn scale(&self, factor: i64) -> i64 { // round up
        if self.0 > 0 {
            (self.0 - 1) / factor + 1
        } else {
            self.0 / factor
        }
    }

    pub fn to_ns(&self) -> i64 { self.scale(1) }
    pub fn to_μs(&self) -> i64 { self.scale(1_000) }
    pub fn to_us(&self) -> i64 { self.to_μs() }
    pub fn to_ms(&self) -> i64 { self.scale(1_000_000) }
    pub fn to_s(&self) -> i64 { self.scale(1_000_000_000) }
    pub fn to_min(&self) -> i64 { self.scale(60_000_000_000) }
    pub fn to_h(&self) -> i64 { self.scale(3_600_000_000_000) }
    pub fn to_days(&self) -> i64 { self.scale(86_400_000_000_000) }
    pub fn to_f64(&self) -> f64 { self.to_ns() as f64 * 1e-9f64 }
} // impl Duration

impl PartialEq for Duration {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
} // impl PartialEq for Duration

impl Eq for Duration {}

impl PartialOrd for Duration {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }

    fn lt(&self, other: &Self) -> bool {
        self.0.lt(&other.0)
    }

    fn le(&self, other: &Self) -> bool {
        self.0.le(&other.0)
    }

    fn gt(&self, other: &Self) -> bool {
        self.0.gt(&other.0)
    }

    fn ge(&self, other: &Self) -> bool {
        self.0.ge(&other.0)
    }
}

impl Ord for Duration {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }

    fn max(self, other: Self) -> Self {
        Duration(self.0.max(other.0))
    }

    fn min(self, other: Self) -> Self {
        Duration(self.0.min(other.0))
    }

    fn clamp(self, min: Self, max: Self) -> Self {
        Duration(self.0.clamp(min.0, max.0))
    }
} // impl Ord for Duration

impl std::ops::Add for Duration {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        let sum = self.0.wrapping_add(other.0);
        assert!(if other.0 < 0 { sum < self.0 } else { sum >= self.0 });
        Duration(sum)
    }
} // impl std::ops::Add for Duration

impl std::ops::Add<Time> for Duration {
    type Output = Time;

    fn add(self, other: Time) -> Time {
        other + self
    }
} // impl std::ops::Add<Time> for Duration

impl std::ops::Sub for Duration {
    type Output = Duration;

    fn sub(self, other: Self) -> Self {
        let diff = self.0.wrapping_sub(other.0);
        assert!(if self.0 >= other.0 { diff >= 0 } else { diff < 0 });
        Duration(diff)
    }
} // impl std::ops::Sub for Duration

impl std::ops::Mul<i64> for Duration {
    type Output = Duration;

    fn mul(self, other: i64) -> Duration {
        let product = self.0.wrapping_mul(other);
        assert!(other == 0 || product / other == self.0);
        Duration(product)
    }
} // impl std::ops::Mul<i64> for Duration

impl std::ops::Mul<Duration> for i64 {
    type Output = Duration;

    fn mul(self, other: Duration) -> Duration {
        let product = self.wrapping_mul(other.0);
        assert!(other.0 == 0 || product / other.0 == self);
        Duration(product)
    }
} // impl std::ops::Mul<Duration> for i64

impl std::fmt::Display for Duration {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        r3::format_in_threes_signed(f, self.0)
    }
} // impl std::fmt::Display for Duration

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
    expires: Time,
    uid: UID,
    kind: TimerKind,
    action: Option<Action>,
    stack_trace: Option<Vec<usize>>,
}

impl std::fmt::Debug for TimerBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimerBody")
            .field("expires", &self.expires)
            .field("uid", &self.uid)
            .field("kind", &self.kind)
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

    pub fn upped<F>(&self, f: F) where F: Fn(&Timer) {
        match self.upgrade() {
            Some(timer) => { f(&timer); }
            None => {
                TRACE!(ATEN_TIMER_UPPED_MISS { TIMER: self });
            }
        };
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

impl std::fmt::Display for Timer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0.uid)
    }
} // impl std::fmt::Display for Timer

type WeakTimer = Timer;

#[derive(Debug)]
struct DiskBody {
    uid: UID,
    poll_fd: RawFd,
    immediate: LinkedList<Rc<RefCell<TimerBody>>>,
    timers: BTreeMap<(Time, UID), Rc<RefCell<TimerBody>>>,
    registrations: HashMap<RawFd, Event>,
    quit: bool,
    wakeup_fd: Option<RawFd>,
    recent: Time,
}

impl Drop for DiskBody {
    fn drop(&mut self) {
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
            TRACE!(ATEN_DISK_EPOLL_CREATE_FAILED { ERR: err });
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
            recent: Time::immemorial(),
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

    pub fn now(&self) -> Time {
        let t = Time::now();
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

    fn new_timer(&self, uid: UID, kind: TimerKind, expires: Time, action: Action)
                 -> (Timer, Rc<RefCell<TimerBody>>) {
        self.wake_up();
        let timer_ref = Rc::new(RefCell::new(TimerBody {
            disk_ref: self.downgrade(),
            expires: expires,
            uid: uid,
            kind: kind,
            action: Some(action),
            stack_trace: None,  // TODO
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
            DISK: self, TIMER: timer_uid, EXPIRES: now,
            ACTION: action_to_string(&action),
        });
        let (timer, timer_ref) = self.new_timer(
            timer_uid, TimerKind::Pending, now, action);
        self.mut_body().immediate.push_back(timer_ref);
        timer
    }

    pub fn schedule(&self, expires: Time, action: Action) -> Timer {
        let timer_uid = UID::new();
        TRACE!(ATEN_DISK_SCHEDULE {
            DISK: self, TIMER: timer_uid, EXPIRES: expires,
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
        let event_body = EventBody {
            disk_ref: self.downgrade(),
            uid: event_uid,
            state: EventState::Idle,
            action: Some(action),
            stack_trace: None,  // TODO
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
                            //if (TRACE_ENABLED!(ATEN_TIMER_BT) &&
                            //    timer->stack_trace)
                            //    emit_timer_backtrace(timer);
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
                        DISK: self, EXPIRY: expiry
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

    fn milliseconds_remaining(&self, until: Time, cap: Option<Duration>) -> i32 {
        let mut duration = until - self.now();
        if duration.elapsed() {
            return 0;
        }
        if let Some(max_duration) = cap {
            if max_duration < duration {
                duration = max_duration;
            }
        }
        let ms = duration.to_ms();
        if ms > i32::MAX as i64 {
            i32::MAX
        } else {
            ms as i32
        }
    }

    fn sleep(&self, until: Time) -> Result<()> {
        if let Err(err) = epoll_wait(
            self.fd(), &mut vec![], self.milliseconds_remaining(until, None)) {
            TRACE!(ATEN_DISK_SLEEP_FAIL { DISK: self });
            return Err(err);
        }
        Ok(())
    }

    fn try_io(&self, next_expiry: Time) -> Result<Option<Time>> {
        let body = self.body();
        let mut epoll_events = vec![libc::epoll_event {
            events: 0,
            u64: 0,
        }];
        match epoll_wait(self.fd(), &mut epoll_events, 0) {
            Err(err) => {
                TRACE!(ATEN_DISK_POLL_FAIL { DISK: self, ERR: err });
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

    pub fn poll(&self) -> Result<Option<Time>> {
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

    fn take_immediate_action(&self) -> Option<Time> {
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
                    TRACE!(ATEN_DISK_LOOP_FAIL { DISK: self, ERR: err });
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
        self.do_loop(noop.clone(), noop.clone(), noop)
    }

    fn prepare_protected_loop(&self) -> Result<RawFd> {
        let mut pipe_fds = vec![0 as libc::c_int; 2];
        let status = unsafe {
            libc::pipe2(pipe_fds.as_mut_ptr(), libc::O_CLOEXEC)
        };
        if status < 0 {
            let err = Error::last_os_error();
            TRACE!(ATEN_DISK_PROTECTED_LOOP_FAIL { DISK: self, ERR: err });
            return Err(err);
        }
        TRACE!(ATEN_DISK_PROTECTED_LOOP { DISK: self });
        let write_fd = pipe_fds[1] as RawFd;
        self.mut_body().wakeup_fd = Some(write_fd);
        Ok(pipe_fds[0] as RawFd)
    }

    fn finish_protected_loop(&self, read_fd: RawFd) -> Result<()> {
        TRACE!(ATEN_DISK_PROTECTED_LOOP_FINISH { DISK: self });
        self.unregister(read_fd)?;
        let mut body = self.mut_body();
        let wakup_fd =
            if let Some(fd) = body.wakeup_fd {
                fd
            } else {
                panic!("wakeup_fd unset")
            };
        unsafe { libc::close(read_fd) };
        unsafe { libc::close(wakup_fd) };
        body.wakeup_fd = None;
        Ok(())
    }

    pub fn protected_loop(&self, lock: Action, unlock: Action) -> Result<()> {
        assert!(self.body().wakeup_fd.is_none());
        let read_fd = self.prepare_protected_loop()?;
        self.do_loop(
            lock,
            unlock,
            Rc::new(move || { drain(read_fd); }))?;
        self.finish_protected_loop(read_fd)
    }

    fn register_with_flags(&self, fd: RawFd, flags: u32, action: Action)
                           -> Result<()> {
        if let Err(err) = nonblock(fd) {
            TRACE!(ATEN_DISK_REGISTER_NONBLOCK_FAIL {
                DISK: self, FD: fd, FLAGS: r3::hex(flags as u64),
                ACTION: action_to_string(&action), ERR: err,
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
                ACTION: action_to_string(&action), ERR: err,
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
        Ok(())
    }

    pub fn register(&self, fd: RawFd, action: Action) -> Result<()> {
        self.register_with_flags(
            fd,
            (libc::EPOLLIN | libc::EPOLLOUT | libc::EPOLLET) as u32,
            action)
    }

    pub fn register_old_school(&self, fd: RawFd, action: Action) -> Result<()> {
        self.register_with_flags(
            fd,
            libc::EPOLLIN as u32,
            action)
    }

    pub fn modify_old_school(&self, fd: RawFd, readable: bool, writable: bool)
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
                ERR: err
            });
            return Err(err);
        }
        TRACE!(ATEN_DISK_MODIFY_OLD_SCHOOL {
            DISK: self, FD: fd, READABLE: readable, WRITABLE: writable,
        });
        self.wake_up();
        Ok(())
    }

    pub fn unregister(&self, fd: RawFd) -> Result<()> {
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
            TRACE!(ATEN_DISK_UNREGISTER_FAIL { DISK: self, FD: fd, ERR: err });
            return Err(err);
        }
        TRACE!(ATEN_DISK_UNREGISTER { DISK: self, FD: fd });
        Ok(())
    }

    pub fn flush(&self, expires: Time) -> Result<()> {
        TRACE!(ATEN_DISK_FLUSH { DISK: self, EXPIRES: expires });
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
                    TRACE!(ATEN_DISK_FLUSH_FAIL { DISK: self, ERR: err });
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

    pub fn in_days(&self, n: i64) -> Time { self.now() + Duration::from_days(n) }
    pub fn in_h(&self, n: i64) -> Time { self.now() + Duration::from_h(n) }
    pub fn in_min(&self, n: i64) -> Time { self.now() + Duration::from_min(n) }
    pub fn in_s(&self, n: i64) -> Time { self.now() + Duration::from_s(n) }
    pub fn in_ms(&self, n: i64) -> Time { self.now() + Duration::from_ms(n) }
    pub fn in_μs(&self, n: i64) -> Time { self.now() + Duration::from_μs(n) }
    pub fn in_ns(&self, n: i64) -> Time { self.now() + Duration::from_ns(n) }
    pub fn in_f64(&self, x: f64) -> Time { self.now() + Duration::from_f64(x) }
} // impl Disk

impl std::fmt::Display for Disk {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0.uid)
    }
} // impl std::fmt::Display for Disk

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

    pub fn upped<F>(&self, f: F) where F: Fn(&Disk) {
        match self.upgrade() {
            Some(disk) => { f(&disk); }
            None => {
                TRACE!(ATEN_DISK_UPPED_MISS { DISK: self.0.uid });
            }
        };
    }
} // impl WeakDisk

#[derive(Debug)]
enum NextStep {
    ImmediateAction,
    TimerExpired(Time, UID),
    NextTimerExpiry(Time),
    InfiniteWait,
}

enum PoppedTimer {
    TimerExpired(Action),
    NextTimerExpiry(Time),
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
    stack_trace: Option<Vec<usize>>,
}

impl std::fmt::Debug for EventBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBody")
            .field("uid", &self.uid)
            .field("state", &self.state)
            .field("stack_trace", &self.stack_trace)
            .finish()
    }
}

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

impl std::fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0.uid)
    }
} // impl std::fmt::Display for Event

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

    pub fn upped<F>(&self, f: F) where F: Fn(&Event) {
        match self.upgrade() {
            Some(event) => { f(&event); }
            None => {
                TRACE!(ATEN_EVENT_UPPED_MISS { EVENT: self.0.uid });
            }
        };
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
