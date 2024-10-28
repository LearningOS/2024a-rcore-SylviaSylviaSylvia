//! Types related to task management

use super::TaskContext;

/// The task control block (TCB) of a task.

//syscall_times:
//0:sys_write()
//1:sys_exit()
//2:sys_yield()
//3:sys_get_time()
//4:sys_task_info()
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
     /// The time when the task first started
    pub start_time: usize,
    /// Indicates whether this is the first run of the task
    pub first_run: bool,
    /// Array to store the counts of system calls made by the task
    pub syscall_times:[u32;5],
}

/// The status of a task
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Exited,
}
