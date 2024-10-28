//! Process management syscalls
use crate::{
    config::MAX_SYSCALL_NUM,
    task::{exit_current_and_run_next, suspend_current_and_run_next, TaskStatus,TASK_MANAGER},
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}
//群友atomicity_wish: syscall_times: [u32;MAX_SYSCALL_NUM]的意思是系统调用次数的类型为大小为MAX_SYSCALL_NUM的u32数组
/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("[kernel] Application exited with code {}", exit_code);
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    inner.tasks[current].syscall_times[1] += 1;
    drop(inner);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    inner.tasks[current].syscall_times[2] += 1;
    drop(inner);
    suspend_current_and_run_next();
    0
}

/// get time with second and microsecond
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    inner.tasks[current].syscall_times[3] += 1;
    drop(inner);
    let us = get_time_us();
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info");
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    inner.tasks[current].syscall_times[4] += 1;

    let start_time = inner.tasks[current].start_time;
    let current_time = get_time_us()/ 1000;

    unsafe{
        (*(_ti)).status = inner.tasks[current].task_status;
        for i in 0..MAX_SYSCALL_NUM{
            if i != 64 && i != 93 && i != 124 && i != 169 && i != 410{
                (*(_ti)).syscall_times[i] = 0;
            }
        }
        (*(_ti)).syscall_times[64] = inner.tasks[current].syscall_times[0];
        (*(_ti)).syscall_times[93] = inner.tasks[current].syscall_times[1];
        (*(_ti)).syscall_times[124] = inner.tasks[current].syscall_times[2];
        (*(_ti)).syscall_times[169] = inner.tasks[current].syscall_times[3];
        (*(_ti)).syscall_times[410] = inner.tasks[current].syscall_times[4];
        (*(_ti)).time = current_time - start_time;
    }
    drop(inner);
    0
}
