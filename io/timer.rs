// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

Synchronous Timers

This module exposes the functionality to create timers, block the current task,
and create receivers which will receive notifications after a period of time.

*/

use comm::{Receiver, Sender, channel};
use time::Duration;
use io::{IoResult, IoError};
use kinds::Send;
use boxed::Box;
use rt::rtio::{IoFactory, LocalIo, RtioTimer, Callback};

/// A synchronous timer object
///
/// Values of this type can be used to put the current task to sleep for a
/// period of time. Handles to this timer can also be created in the form of
/// receivers which will receive notifications over time.
///
/// # Example
///
/// ```
/// # fn main() {}
/// # fn foo() {
/// use std::io::Timer;
///
/// let mut timer = Timer::new().unwrap();
/// timer.sleep(10); // block the task for awhile
///
/// let timeout = timer.oneshot(10);
/// // do some work
/// timeout.recv(); // wait for the timeout to expire
///
/// let periodic = timer.periodic(10);
/// loop {
///     periodic.recv();
///     // this loop is only executed once every 10ms
/// }
/// # }
/// ```
///
/// If only sleeping is necessary, then a convenience API is provided through
/// the `io::timer` module.
///
/// ```
/// # fn main() {}
/// # fn foo() {
/// use std::io::timer;
///
/// // Put this task to sleep for 5 seconds
/// timer::sleep(5000);
/// # }
/// ```
pub struct Timer {
    obj: Box<RtioTimer + Send>,
}

struct TimerCallback { tx: Sender<()> }

fn in_ms(d: Duration) -> u64 {
    // FIXME: Do we really want to fail on negative duration?
    let ms = d.num_milliseconds();
    if ms < 0 { fail!("negative duration") }
    return ms as u64;
}

/// Sleep the current task for the specified duration.
pub fn sleep(duration: Duration) {
    sleep_ms(in_ms(duration))
}

/// Sleep the current task for `msecs` milliseconds.
pub fn sleep_ms(msecs: u64) {
    let timer = Timer::new();
    let mut timer = timer.ok().expect("timer::sleep: could not create a Timer");

    timer.sleep_ms(msecs)
}

impl Timer {
    /// Creates a new timer which can be used to put the current task to sleep
    /// for a number of milliseconds, or to possibly create channels which will
    /// get notified after an amount of time has passed.
    pub fn new() -> IoResult<Timer> {
        LocalIo::maybe_raise(|io| {
            io.timer_init().map(|t| Timer { obj: t })
        }).map_err(IoError::from_rtio_error)
    }

    /// Blocks the current task for the specified duration.
    ///
    /// Note that this function will cause any other receivers for this timer to
    /// be invalidated (the other end will be closed).
    pub fn sleep(&mut self, duration: Duration) {
        self.obj.sleep(in_ms(duration));
    }

    /// Blocks the current task for `msecs` milliseconds.
    ///
    /// Note that this function will cause any other receivers for this timer to
    /// be invalidated (the other end will be closed).
    pub fn sleep_ms(&mut self, msecs: u64) {
        self.obj.sleep(msecs);
    }

    /// Creates a oneshot receiver which will have a notification sent when
    /// the specified duration has elapsed.
    ///
    /// This does *not* block the current task, but instead returns immediately.
    ///
    /// Note that this invalidates any previous receiver which has been created
    /// by this timer, and that the returned receiver will be invalidated once
    /// the timer is destroyed (when it falls out of scope). In particular, if
    /// this is called in method-chaining style, the receiver will be
    /// invalidated at the end of that statement, and all `recv` calls will
    /// fail.
    pub fn oneshot(&mut self, duration: Duration) -> Receiver<()> {
        let (tx, rx) = channel();
        self.obj.oneshot(in_ms(duration), box TimerCallback { tx: tx });
        return rx
    }

    /// Creates a oneshot receiver which will have a notification sent when
    /// `msecs` milliseconds has elapsed.
    ///
    /// This does *not* block the current task, but instead returns immediately.
    ///
    /// Note that this invalidates any previous receiver which has been created
    /// by this timer, and that the returned receiver will be invalidated once
    /// the timer is destroyed (when it falls out of scope). In particular, if
    /// this is called in method-chaining style, the receiver will be
    /// invalidated at the end of that statement, and all `recv` calls will
    /// fail.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::io::Timer;
    ///
    /// let mut timer = Timer::new().unwrap();
    /// let ten_milliseconds = timer.oneshot(10);
    ///
    /// for _ in range(0u, 100) { /* do work */ }
    ///
    /// // blocks until 10 ms after the `oneshot` call
    /// ten_milliseconds.recv();
    /// ```
    ///
    /// ```rust
    /// use std::io::Timer;
    ///
    /// // Incorrect, method chaining-style:
    /// let mut five_ms = Timer::new().unwrap().oneshot(5);
    /// // The timer object was destroyed, so this will always fail:
    /// // five_ms.recv()
    /// ```
    pub fn oneshot_ms(&mut self, msecs: u64) -> Receiver<()> {
        let (tx, rx) = channel();
        self.obj.oneshot(msecs, box TimerCallback { tx: tx });
        return rx
    }

    /// Creates a receiver which will have a continuous stream of notifications
    /// being sent each time the specified duration has elapsed.
    ///
    /// This does *not* block the current task, but instead returns
    /// immediately. The first notification will not be received immediately,
    /// but rather after the first duration.
    ///
    /// Note that this invalidates any previous receiver which has been created
    /// by this timer, and that the returned receiver will be invalidated once
    /// the timer is destroyed (when it falls out of scope). In particular, if
    /// this is called in method-chaining style, the receiver will be
    /// invalidated at the end of that statement, and all `recv` calls will
    /// fail.
    pub fn periodic(&mut self, duration: Duration) -> Receiver<()> {
        let (tx, rx) = channel();
        self.obj.period(in_ms(duration), box TimerCallback { tx: tx });
        return rx
    }

    /// Creates a receiver which will have a continuous stream of notifications
    /// being sent every `msecs` milliseconds.
    ///
    /// This does *not* block the current task, but instead returns
    /// immediately. The first notification will not be received immediately,
    /// but rather after `msec` milliseconds have passed.
    ///
    /// Note that this invalidates any previous receiver which has been created
    /// by this timer, and that the returned receiver will be invalidated once
    /// the timer is destroyed (when it falls out of scope). In particular, if
    /// this is called in method-chaining style, the receiver will be
    /// invalidated at the end of that statement, and all `recv` calls will
    /// fail.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::io::Timer;
    ///
    /// let mut timer = Timer::new().unwrap();
    /// let ten_milliseconds = timer.periodic(10);
    ///
    /// for _ in range(0u, 100) { /* do work */ }
    ///
    /// // blocks until 10 ms after the `periodic` call
    /// ten_milliseconds.recv();
    ///
    /// for _ in range(0u, 100) { /* do work */ }
    ///
    /// // blocks until 20 ms after the `periodic` call (*not* 10ms after the
    /// // previous `recv`)
    /// ten_milliseconds.recv();
    /// ```
    ///
    /// ```rust
    /// use std::io::Timer;
    ///
    /// // Incorrect, method chaining-style.
    /// let mut five_ms = Timer::new().unwrap().periodic(5);
    /// // The timer object was destroyed, so this will always fail:
    /// // five_ms.recv()
    /// ```
    pub fn periodic_ms(&mut self, msecs: u64) -> Receiver<()> {
        let (tx, rx) = channel();
        self.obj.period(msecs, box TimerCallback { tx: tx });
        return rx
    }
}

impl Callback for TimerCallback {
    fn call(&mut self) {
        let _ = self.tx.send_opt(());
    }
}

#[cfg(test)]
mod test {
    iotest!(fn test_io_timer_sleep_ms_simple() {
        let mut timer = Timer::new().unwrap();
        timer.sleep_ms(1);
    })

    iotest!(fn test_io_timer_sleep_oneshot_ms() {
        let mut timer = Timer::new().unwrap();
        timer.oneshot_ms(1).recv();
    })

    iotest!(fn test_io_timer_sleep_oneshot_ms_forget() {
        let mut timer = Timer::new().unwrap();
        timer.oneshot_ms(100000000000);
    })

    iotest!(fn oneshot_ms_twice() {
        let mut timer = Timer::new().unwrap();
        let rx1 = timer.oneshot_ms(10000);
        let rx = timer.oneshot_ms(1);
        rx.recv();
        assert_eq!(rx1.recv_opt(), Err(()));
    })

    iotest!(fn test_io_timer_oneshot_ms_then_sleep() {
        let mut timer = Timer::new().unwrap();
        let rx = timer.oneshot_ms(100000000000);
        timer.sleep_ms(1); // this should invalidate rx

        assert_eq!(rx.recv_opt(), Err(()));
    })

    iotest!(fn test_io_timer_sleep_periodic_ms() {
        let mut timer = Timer::new().unwrap();
        let rx = timer.periodic_ms(1);
        rx.recv();
        rx.recv();
        rx.recv();
    })

    iotest!(fn test_io_timer_sleep_periodic_ms_forget() {
        let mut timer = Timer::new().unwrap();
        timer.periodic_ms(100000000000);
    })

    iotest!(fn test_io_timer_sleep_ms_standalone() {
        sleep_ms(1)
    })

    iotest!(fn oneshot_ms() {
        let mut timer = Timer::new().unwrap();

        let rx = timer.oneshot_ms(1);
        rx.recv();
        assert!(rx.recv_opt().is_err());

        let rx = timer.oneshot_ms(1);
        rx.recv();
        assert!(rx.recv_opt().is_err());
    })

    iotest!(fn override() {
        let mut timer = Timer::new().unwrap();
        let orx = timer.oneshot_ms(100);
        let prx = timer.periodic_ms(100);
        timer.sleep_ms(1);
        assert_eq!(orx.recv_opt(), Err(()));
        assert_eq!(prx.recv_opt(), Err(()));
        timer.oneshot_ms(1).recv();
    })

    iotest!(fn period_ms() {
        let mut timer = Timer::new().unwrap();
        let rx = timer.periodic_ms(1);
        rx.recv();
        rx.recv();
        let rx2 = timer.periodic_ms(1);
        rx2.recv();
        rx2.recv();
    })

    iotest!(fn sleep_ms() {
        let mut timer = Timer::new().unwrap();
        timer.sleep_ms(1);
        timer.sleep_ms(1);
    })

    iotest!(fn oneshot_ms_fail() {
        let mut timer = Timer::new().unwrap();
        let _rx = timer.oneshot_ms(1);
        fail!();
    } #[should_fail])

    iotest!(fn period_ms_fail() {
        let mut timer = Timer::new().unwrap();
        let _rx = timer.periodic_ms(1);
        fail!();
    } #[should_fail])

    iotest!(fn normal_fail() {
        let _timer = Timer::new().unwrap();
        fail!();
    } #[should_fail])

    iotest!(fn closing_channel_during_drop_doesnt_kill_everything() {
        // see issue #10375
        let mut timer = Timer::new().unwrap();
        let timer_rx = timer.periodic_ms(1000);

        spawn(proc() {
            let _ = timer_rx.recv_opt();
        });

        // when we drop the TimerWatcher we're going to destroy the channel,
        // which must wake up the task on the other end
    })

    iotest!(fn reset_doesnt_switch_tasks() {
        // similar test to the one above.
        let mut timer = Timer::new().unwrap();
        let timer_rx = timer.periodic_ms(1000);

        spawn(proc() {
            let _ = timer_rx.recv_opt();
        });

        timer.oneshot_ms(1);
    })

    iotest!(fn reset_doesnt_switch_tasks2() {
        // similar test to the one above.
        let mut timer = Timer::new().unwrap();
        let timer_rx = timer.periodic_ms(1000);

        spawn(proc() {
            let _ = timer_rx.recv_opt();
        });

        timer.sleep_ms(1);
    })

    iotest!(fn sender_goes_away_oneshot() {
        let rx = {
            let mut timer = Timer::new().unwrap();
            timer.oneshot_ms(1000)
        };
        assert_eq!(rx.recv_opt(), Err(()));
    })

    iotest!(fn sender_goes_away_period() {
        let rx = {
            let mut timer = Timer::new().unwrap();
            timer.periodic_ms(1000)
        };
        assert_eq!(rx.recv_opt(), Err(()));
    })

    iotest!(fn receiver_goes_away_oneshot() {
        let mut timer1 = Timer::new().unwrap();
        timer1.oneshot_ms(1);
        let mut timer2 = Timer::new().unwrap();
        // while sleeping, the previous timer should fire and not have its
        // callback do something terrible.
        timer2.sleep_ms(2);
    })

    iotest!(fn receiver_goes_away_period() {
        let mut timer1 = Timer::new().unwrap();
        timer1.periodic_ms(1);
        let mut timer2 = Timer::new().unwrap();
        // while sleeping, the previous timer should fire and not have its
        // callback do something terrible.
        timer2.sleep_ms(2);
    })


    iotest!(fn test_io_timer_sleep_duration_simple() {
        use time::Duration;
        let mut timer = Timer::new().unwrap();
        timer.sleep(Duration::seconds(1));
    })

    iotest!(fn test_io_timer_sleep_oneshot_duration() {
        use time::Duration;
        let mut timer = Timer::new().unwrap();
        timer.oneshot(Duration::seconds(1)).recv();
    })

    iotest!(fn test_io_timer_sleep_periodic_duration() {
        use time::Duration;
        let mut timer = Timer::new().unwrap();
        let rx = timer.periodic(Duration::seconds(1));
        rx.recv();
        rx.recv();
        rx.recv();
    })


}
