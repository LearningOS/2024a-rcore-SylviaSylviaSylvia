//! Mutex (spin-like and blocking(sleep))

use super::UPSafeCell;
use crate::task::TaskControlBlock;
use crate::task::{block_current_and_run_next, suspend_current_and_run_next};
use crate::task::{current_task, wakeup_task};
use crate::task::current_process;
use alloc::{collections::VecDeque, sync::Arc};

/// Mutex trait
pub trait Mutex: Sync + Send {
    /// Lock the mutex
    fn lock(&self) -> isize;
    /// Unlock the mutex
    fn unlock(&self);
}

/// Spinlock Mutex struct
pub struct MutexSpin {
    locked: UPSafeCell<bool>,
}

impl MutexSpin {
    /// Create a new spinlock mutex
    pub fn new() -> Self {
        Self {
            locked: unsafe { UPSafeCell::new(false) },
        }
    }
}

impl Mutex for MutexSpin {
    /// Lock the spinlock mutex
    fn lock(&self) -> isize{
        trace!("kernel: MutexSpin::lock");
        loop {
            let mut locked = self.locked.exclusive_access();
            if *locked {
                let current = current_process();
                let mut current_inner = current.inner_exclusive_access();
                current_inner.deadlocked = true;
                let mut exit_now = false;
                if current_inner.enable_deadlock_detect == true{
                    exit_now = true;
                }
                drop(current_inner);
                drop(current);
                drop(locked);
                if exit_now == true {
                    return -0xDEAD;
                }
                suspend_current_and_run_next();
                continue;
            } else {
                *locked = true;
                drop(locked);
                return 0;
            }
        }
    }

    fn unlock(&self) {
        trace!("kernel: MutexSpin::unlock");
        let mut locked = self.locked.exclusive_access();
        *locked = false;
        let current = current_process();
        let mut current_inner = current.inner_exclusive_access();
        current_inner.deadlocked = false;
        drop(current_inner);
        drop(current);
        drop(locked);
    }
}

/// Blocking Mutex struct
pub struct MutexBlocking {
    inner: UPSafeCell<MutexBlockingInner>,
}

pub struct MutexBlockingInner {
    locked: bool,
    wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl MutexBlocking {
    /// Create a new blocking mutex
    pub fn new() -> Self {
        trace!("kernel: MutexBlocking::new");
        Self {
            inner: unsafe {
                UPSafeCell::new(MutexBlockingInner {
                    locked: false,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }
}

impl Mutex for MutexBlocking {
    /// lock the blocking mutex
    fn lock(&self) -> isize{
        trace!("kernel: MutexBlocking::lock");
        let mut mutex_inner = self.inner.exclusive_access();
        if mutex_inner.locked {
            let current = current_process();
            let mut current_inner = current.inner_exclusive_access();
            current_inner.deadlocked = true;
            let mut exit_now = false;
            if current_inner.enable_deadlock_detect == true {
                exit_now = true;
            }
            drop(current_inner);
            drop(current);
            if exit_now == true {
                return -0xDEAD;
            }
            mutex_inner.wait_queue.push_back(current_task().unwrap());
            drop(mutex_inner);
            block_current_and_run_next();
        } else {
            mutex_inner.locked = true;
            drop(mutex_inner);
        }
        0
    }

    /// unlock the blocking mutex
    fn unlock(&self) {
        trace!("kernel: MutexBlocking::unlock");
        let mut mutex_inner = self.inner.exclusive_access();
        assert!(mutex_inner.locked);
        let current = current_process();
        let mut current_inner = current.inner_exclusive_access();
        current_inner.deadlocked = false;
        drop(current_inner);
        drop(current);
        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
            wakeup_task(waking_task);
        } else {
            mutex_inner.locked = false;
        }
        drop(mutex_inner);
    }
}
