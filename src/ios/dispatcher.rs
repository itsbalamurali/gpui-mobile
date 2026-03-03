//! iOS task dispatcher using Grand Central Dispatch (GCD).
//!
//! iOS shares the same GCD infrastructure as macOS, so this implementation
//! is nearly identical to the macOS dispatcher.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use gpui::{PlatformDispatcher, Priority, RunnableVariant, ThreadTaskTimings};
use std::thread;

use objc::{
    class, msg_send,
    runtime::{BOOL, YES},
    sel, sel_impl,
};
use std::{ffi::c_void, ptr::NonNull, time::Duration};

// GCD types - these are the same on iOS and macOS
type dispatch_queue_t = *mut std::ffi::c_void;
type dispatch_time_t = u64;

const DISPATCH_TIME_NOW: dispatch_time_t = 0;
const DISPATCH_QUEUE_PRIORITY_HIGH: i64 = 2;
const DISPATCH_QUEUE_PRIORITY_DEFAULT: i64 = 0;
const DISPATCH_QUEUE_PRIORITY_LOW: i64 = -2;
const DISPATCH_QUEUE_PRIORITY_BACKGROUND: i64 = -32768;

// SAFETY: These are C functions from libdispatch
unsafe extern "C" {
    static _dispatch_main_q: std::ffi::c_void;
    fn dispatch_async_f(
        queue: dispatch_queue_t,
        context: *mut c_void,
        work: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn dispatch_after_f(
        when: dispatch_time_t,
        queue: dispatch_queue_t,
        context: *mut c_void,
        work: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn dispatch_get_global_queue(identifier: i64, flags: u64) -> dispatch_queue_t;
    fn dispatch_time(when: dispatch_time_t, delta: i64) -> dispatch_time_t;
}

pub(crate) fn dispatch_get_main_queue() -> dispatch_queue_t {
    std::ptr::addr_of!(_dispatch_main_q) as *const _ as dispatch_queue_t
}

fn priority_to_gcd(priority: Priority) -> i64 {
    match priority {
        Priority::High => DISPATCH_QUEUE_PRIORITY_HIGH,
        Priority::Low => DISPATCH_QUEUE_PRIORITY_LOW,
        _ => DISPATCH_QUEUE_PRIORITY_DEFAULT,
    }
}

pub(crate) struct IosDispatcher;

impl PlatformDispatcher for IosDispatcher {
    fn is_main_thread(&self) -> bool {
        unsafe {
            let is_main: BOOL = msg_send![class!(NSThread), isMainThread];
            is_main == YES
        }
    }

    fn get_all_timings(&self) -> Vec<ThreadTaskTimings> {
        Vec::new()
    }

    fn get_current_thread_timings(&self) -> ThreadTaskTimings {
        ThreadTaskTimings {
            thread_name: None,
            thread_id: thread::current().id(),
            timings: Vec::new(),
            total_pushed: 0,
        }
    }

    fn dispatch(&self, runnable: RunnableVariant, priority: Priority) {
        let context = runnable.into_raw().as_ptr() as *mut c_void;
        unsafe {
            dispatch_async_f(
                dispatch_get_global_queue(priority_to_gcd(priority), 0),
                context,
                Some(trampoline),
            );
        }
    }

    fn dispatch_on_main_thread(&self, runnable: RunnableVariant, _priority: Priority) {
        let context = runnable.into_raw().as_ptr() as *mut c_void;
        unsafe {
            dispatch_async_f(dispatch_get_main_queue(), context, Some(trampoline));
        }
    }

    fn dispatch_after(&self, duration: Duration, runnable: RunnableVariant) {
        let context = runnable.into_raw().as_ptr() as *mut c_void;
        unsafe {
            let queue = dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_HIGH, 0);
            let when = dispatch_time(DISPATCH_TIME_NOW, duration.as_nanos() as i64);
            dispatch_after_f(when, queue, context, Some(trampoline));
        }
    }

    fn spawn_realtime(&self, f: Box<dyn FnOnce() + Send>) {
        // On iOS, we don't have direct realtime thread control like macOS.
        // Use a high-priority GCD queue as an approximation.
        thread::Builder::new()
            .name("gpui-ios-realtime".into())
            .spawn(move || {
                f();
            })
            .ok();
    }
}

unsafe extern "C" fn trampoline(runnable: *mut c_void) {
    let task = unsafe { RunnableVariant::from_raw(NonNull::new_unchecked(runnable as *mut ())) };
    task.run();
}
