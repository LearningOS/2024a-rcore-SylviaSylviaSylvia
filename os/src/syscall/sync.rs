use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task, suspend_current_and_run_next};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::vec;

/// sleep for some ms
pub fn sleep(period_ms: usize) {
    let start = get_time_ms();
    while get_time_ms() < start + period_ms {
        suspend_current_and_run_next();
    }
}

/// ResourceAllocation
pub struct ResourceAllocation {
    /// available resources
    pub available: Vec<usize>,
    /// allocated resources
    pub allocation :Vec<Vec<usize>>,
    /// resources still needed
    pub need: Vec<Vec<usize>>,
    /// Store the tid corresponding to the i-th thread
    pub tid_list: Vec<usize>,
    /// Finish a thread?
    pub finish: Vec<bool>,
    /// available + will available when thread release
    pub maximum: Vec<usize>,
}

impl ResourceAllocation {
    /// initialize
    pub fn new() -> Self {
        Self{
            // resources: lock, semaphore and condvar
            available: Vec::new(),
            allocation: vec![Vec::new()],
            need: vec![Vec::new()],
            tid_list: Vec::new(),
            finish: Vec::new(),
            maximum: Vec::new(),
        }
    }
    /// add resources types
    pub fn add_res_type(&mut self, res_count: usize)
    {
        self.available.push(res_count);
        self.maximum.push(res_count);
        for i in 0..self.tid_list.len(){
            self.allocation[i].push(0);
            self.need[i].push(0);
        }
    }
    /// the num of resources types
    pub fn resources_types_num(&self) -> usize {
        self.available.len()
    }
    /// find the information corresponding to a thread, -1 means not found
    pub fn find(&self, tid: usize) -> isize {
        let mut idx: isize = -1;
        for i in 0..self.tid_list.len(){
            if self.tid_list[i] == tid {
                idx = i as isize;
                break;
            }
        }
        idx
    }
    /// add a thread
    pub fn addthread(&mut self, tid: usize){
        let j = self.available.len();
        self.allocation.push(vec![0;j]);
        self.need.push(vec![0;j]);
        self.tid_list.push(tid);
        self.finish[self.tid_list.len()-1] = false;
    }
    /// allocate resource
    pub fn alloc(&mut self, tid: usize, res_id: usize) {
        let i = self.find(tid) as usize;
        self.need[i][res_id] = 1;
        while self.available[res_id] < 1 {
            sleep(10);
        }
        self.available[res_id] -= 1;
        self.allocation[i][res_id] += 1; 
        self.need[i][res_id] = 0;
        self.finish[i] = true;
        self.renew_maximum(tid);
    }
    /// deallocate resource
    pub fn dealloc(&mut self, tid: usize, res_id: usize){
        let i = self.find(tid) as usize;
        self.allocation[i][res_id] -= 1;
        self.available[res_id] += 1;
    }
    /// allocate finish so resources will be released soon
    pub fn renew_maximum(&mut self, tid: usize){
        let i = self.find(tid) as usize;
        if self.finish[i] == true {
            for j in 0..self.maximum.len(){
                self.maximum[j] += self.allocation[i][j];
            }
        }
        else {
            for j in 0..self.maximum.len(){
                self.maximum[j] -= self.allocation[i][j];
            }
        }
    }
    /// release resources
    pub fn release(&mut self, tid: usize){
        let i = self.find(tid) as usize;
        for j in 0..self.available.len(){
            self.available[j] += self.allocation[i][j];
            self.allocation[i][j] = 0;
        }
    }
    /// allocate resources 
    pub fn is_deadlocked(&mut self,res_id: usize) -> bool {
        let mut result = false;
        if self.maximum[res_id] < 1 {
            result = true;
        }
        result
    }
}

/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_sleep",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        drop(process_inner);
        drop(process);
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        let res = process_inner.mutex_list.len() as isize - 1;
        drop(process_inner);
        drop(process);
        res
    }
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    let res = mutex.lock();
    drop(mutex);
    if res == -0xDEAD{
        return -0xDEAD;
    }
    0
}
/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_unlock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.unlock();
    drop(mutex);
    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_list.len() - 1
    };
    drop(process_inner);
    drop(process);
    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    sem.up();
    drop(sem);
    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    if sem_id > 2{
        return -0xDEAD;
    }
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    sem.down();
    drop(sem);
    0
}
/// condvar create syscall
pub fn sys_condvar_create() -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    drop(process_inner);
    drop(process);
    id as isize
}
/// condvar signal syscall
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_signal",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    condvar.signal();
    drop(condvar);
    0
}
/// condvar wait syscall
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_wait",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    condvar.wait(mutex);
    drop(condvar);
    0
}



/// enable deadlock detection syscall
///
/// YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(enabled: usize) -> isize {
    trace!("kernel: sys_enable_deadlock_detect");
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    if enabled > 0 {
        process_inner.enable_deadlock_detect = true;
    }
    else {
        process_inner.enable_deadlock_detect = false;
    }
    drop(process_inner);
    drop(process);
    0
}

