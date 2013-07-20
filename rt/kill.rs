// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Task death: asynchronous killing, linked failure, exit code propagation.

use cell::Cell;
use option::{Option, Some, None};
use prelude::*;
use unstable::sync::{UnsafeAtomicRcBox, LittleLock};
use util;

// FIXME(#7544)(bblum): think about the cache efficiency of this
struct KillHandleInner {
    // ((more fields to be added in a future commit))

    // Shared state between task and children for exit code propagation. These
    // are here so we can re-use the kill handle to implement watched children
    // tasks. Using a separate ARClike would introduce extra atomic adds/subs
    // into common spawn paths, so this is just for speed.

    // Locklessly accessed; protected by the enclosing refcount's barriers.
    any_child_failed: bool,
    // A lazy list, consuming which may unwrap() many child tombstones.
    child_tombstones: Option<~fn() -> bool>,
    // Protects multiple children simultaneously creating tombstones.
    graveyard_lock: LittleLock,
}

/// State shared between tasks used for task killing during linked failure.
#[deriving(Clone)]
pub struct KillHandle(UnsafeAtomicRcBox<KillHandleInner>);

/// Per-task state related to task death, killing, failure, etc.
pub struct Death {
    // Shared among this task, its watched children, and any linked tasks who
    // might kill it. This is optional so we can take it by-value at exit time.
    kill_handle:     Option<KillHandle>,
    // Handle to a watching parent, if we have one, for exit code propagation.
    watching_parent: Option<KillHandle>,
    // Action to be done with the exit code. If set, also makes the task wait
    // until all its watched children exit before collecting the status.
    on_exit:         Option<~fn(bool)>,
}

impl KillHandle {
    pub fn new() -> KillHandle {
        KillHandle(UnsafeAtomicRcBox::new(KillHandleInner {
            // Linked failure fields
            // ((none yet))
            // Exit code propagation fields
            any_child_failed: false,
            child_tombstones: None,
            graveyard_lock:   LittleLock(),
        }))
    }

    pub fn notify_immediate_failure(&mut self) {
        // A benign data race may happen here if there are failing sibling
        // tasks that were also spawned-watched. The refcount's write barriers
        // in UnsafeAtomicRcBox ensure that this write will be seen by the
        // unwrapper/destructor, whichever task may unwrap it.
        unsafe { (*self.get()).any_child_failed = true; }
    }

    // For use when a task does not need to collect its children's exit
    // statuses, but the task has a parent which might want them.
    pub fn reparent_children_to(self, parent: &mut KillHandle) {
        // Optimistic path: If another child of the parent's already failed,
        // we don't need to worry about any of this.
        if unsafe { (*parent.get()).any_child_failed } {
            return;
        }

        // Try to see if all our children are gone already.
        match unsafe { self.try_unwrap() } {
            // Couldn't unwrap; children still alive. Reparent entire handle as
            // our own tombstone, to be unwrapped later.
            Left(this) => {
                let this = Cell::new(this); // :(
                do add_lazy_tombstone(parent) |other_tombstones| {
                    let this = Cell::new(this.take()); // :(
                    let others = Cell::new(other_tombstones); // :(
                    || {
                        // Prefer to check tombstones that were there first,
                        // being "more fair" at the expense of tail-recursion.
                        others.take().map_consume_default(true, |f| f()) && {
                            let mut inner = unsafe { this.take().unwrap() };
                            (!inner.any_child_failed) &&
                                inner.child_tombstones.take_map_default(true, |f| f())
                        }
                    }
                }
            }
            // Whether or not all children exited, one or more already failed.
            Right(KillHandleInner { any_child_failed: true, _ }) => {
                parent.notify_immediate_failure();
            }
            // All children exited, but some left behind tombstones that we
            // don't want to wait on now. Give them to our parent.
            Right(KillHandleInner { any_child_failed: false,
                                    child_tombstones: Some(f), _ }) => {
                let f = Cell::new(f); // :(
                do add_lazy_tombstone(parent) |other_tombstones| {
                    let f = Cell::new(f.take()); // :(
                    let others = Cell::new(other_tombstones); // :(
                    || {
                        // Prefer fairness to tail-recursion, as in above case.
                        others.take().map_consume_default(true, |f| f()) &&
                            f.take()()
                    }
                }
            }
            // All children exited, none failed. Nothing to do!
            Right(KillHandleInner { any_child_failed: false,
                                    child_tombstones: None, _ }) => { }
        }

        // NB: Takes a pthread mutex -- 'blk' not allowed to reschedule.
        fn add_lazy_tombstone(parent: &mut KillHandle,
                              blk: &fn(Option<~fn() -> bool>) -> ~fn() -> bool) {

            let inner: &mut KillHandleInner = unsafe { &mut *parent.get() };
            unsafe {
                do inner.graveyard_lock.lock {
                    // Update the current "head node" of the lazy list.
                    inner.child_tombstones =
                        Some(blk(util::replace(&mut inner.child_tombstones, None)));
                }
            }
        }
    }
}

impl Death {
    pub fn new() -> Death {
        Death {
            kill_handle:     Some(KillHandle::new()),
            watching_parent: None,
            on_exit:         None,
        }
    }

    pub fn new_child(&self) -> Death {
        // FIXME(#7327)
        Death {
            kill_handle:     Some(KillHandle::new()),
            watching_parent: self.kill_handle.clone(),
            on_exit:         None,
        }
    }

    /// Collect failure exit codes from children and propagate them to a parent.
    pub fn collect_failure(&mut self, mut success: bool) {
        // Step 1. Decide if we need to collect child failures synchronously.
        do self.on_exit.take_map |on_exit| {
            if success {
                // We succeeded, but our children might not. Need to wait for them.
                let mut inner = unsafe { self.kill_handle.take_unwrap().unwrap() };
                if inner.any_child_failed {
                    success = false;
                } else {
                    // Lockless access to tombstones protected by unwrap barrier.
                    success = inner.child_tombstones.take_map_default(true, |f| f());
                }
            }
            on_exit(success);
        };

        // Step 2. Possibly alert possibly-watching parent to failure status.
        // Note that as soon as parent_handle goes out of scope, the parent
        // can successfully unwrap its handle and collect our reported status.
        do self.watching_parent.take_map |mut parent_handle| {
            if success {
                // Our handle might be None if we had an exit callback, and
                // already unwrapped it. But 'success' being true means no
                // child failed, so there's nothing to do (see below case).
                do self.kill_handle.take_map |own_handle| {
                    own_handle.reparent_children_to(&mut parent_handle);
                };
            } else {
                // Can inform watching parent immediately that we failed.
                // (Note the importance of non-failing tasks NOT writing
                // 'false', which could obscure another task's failure.)
                parent_handle.notify_immediate_failure();
            }
        };
    }
}
