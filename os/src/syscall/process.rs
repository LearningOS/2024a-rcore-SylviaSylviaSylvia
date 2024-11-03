//! Process management syscalls
use alloc::sync::Arc;

use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE},
    loader::get_app_data_by_name,
    mm::{translated_refmut, translated_str,VirtPageNum, VirtAddr, MapPermission,
        frame_allocator::FRAME_ALLOCATOR,PhysAddr},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskStatus,TaskControlBlock
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

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
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    let current = current_task().unwrap();
    let mut inner = current.inner_exclusive_access();
    inner.syscall_times[2] += 1;
    drop(inner);
    drop(current);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
    let current = current_task().unwrap();
    let mut inner = current.inner_exclusive_access();
    inner.syscall_times[3] += 1;
    drop(inner);
    drop(current);
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    let current = current_task().unwrap();
    let mut inner = current.inner_exclusive_access();
    inner.syscall_times[6] += 1;
    drop(inner);
    drop(current);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let mut inner = current_task.inner_exclusive_access();
    inner.syscall_times[9] += 1;
    drop(inner);
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let current = current_task().unwrap();
    let mut inner = current.inner_exclusive_access();
    inner.syscall_times[10] += 1;
    drop(inner);
    drop(current);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!("kernel::pid[{}] sys_waitpid [{}]", current_task().unwrap().pid.0, pid);

    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    inner.syscall_times[12] += 1;
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
    
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time",
        current_task().unwrap().pid.0
    );
    let current = current_task().unwrap();
    current.inner_exclusive_access().syscall_times[5] += 1;

    let us = get_time_us();
    let ts_addr = VirtAddr::from(ts as usize);
    let ts_page_num = VirtPageNum::from(ts_addr.floor());
    let ts_offset = ts_addr.page_offset();

    let phys_page_num = current.inner_exclusive_access().memory_set.translate(ts_page_num).unwrap().ppn();
    
    let phys_addr = PhysAddr::from(phys_page_num);
    let phys_ts = (phys_addr.0 + ts_offset) as *mut TimeVal;

    unsafe {
        *(phys_ts) = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    drop(current);
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    trace!(
        "kernel:pid[{}] sys_task_info",
        current_task().unwrap().pid.0
    );
    let current = current_task().unwrap();
    let mut inner = current.inner_exclusive_access();
    inner.syscall_times[14] += 1;

    let start_time = inner.start_time;
    let current_time = get_time_us() / 1000;

    let ti_addr = VirtAddr::from(ti as usize);
    let ti_page_num = VirtPageNum::from(ti_addr.floor());
    let ti_offset = ti_addr.page_offset();
    let phys_page_num = inner.memory_set.translate(ti_page_num).unwrap().ppn();
    let phys_addr = PhysAddr::from(phys_page_num);
    let phys_ti = (phys_addr.0 + ti_offset) as *mut TaskInfo;

    unsafe {
        (*(phys_ti)).status = inner.task_status;
        for i in 0..MAX_SYSCALL_NUM {
            (*(phys_ti)).syscall_times[i] = 0;
        }

        (*(phys_ti)).syscall_times[63] = inner.syscall_times[0];
        (*(phys_ti)).syscall_times[64] = inner.syscall_times[1];
        (*(phys_ti)).syscall_times[93] = inner.syscall_times[2];
        (*(phys_ti)).syscall_times[124] = inner.syscall_times[3];
        (*(phys_ti)).syscall_times[140] = inner.syscall_times[4];
        (*(phys_ti)).syscall_times[169] = inner.syscall_times[5];
        (*(phys_ti)).syscall_times[172] = inner.syscall_times[6];
        (*(phys_ti)).syscall_times[214] = inner.syscall_times[7];
        (*(phys_ti)).syscall_times[215] = inner.syscall_times[8];
        (*(phys_ti)).syscall_times[220] = inner.syscall_times[9];
        (*(phys_ti)).syscall_times[221] = inner.syscall_times[10];
        (*(phys_ti)).syscall_times[222] = inner.syscall_times[11];
        (*(phys_ti)).syscall_times[260] = inner.syscall_times[12];
        (*(phys_ti)).syscall_times[400] = inner.syscall_times[13];
        (*(phys_ti)).syscall_times[410] = inner.syscall_times[14];
        (*(phys_ti)).time = current_time - start_time;
    }
    drop(inner);
    drop(current);
    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap",
        current_task().unwrap().pid.0
    );
    let current = current_task().unwrap();
    let mut inner = current.inner_exclusive_access();
    inner.syscall_times[11] += 1;

    if start & (PAGE_SIZE-1) != 0 {
        return -1;
    }
    if port & !0x7 != 0 || port & 0x7 == 0 {
        return -1;
    }

    let num_pages = (len + PAGE_SIZE - 1) / PAGE_SIZE; 

    let frame_allocator = FRAME_ALLOCATOR.exclusive_access();
    if frame_allocator.end - frame_allocator.current + frame_allocator.recycled.len() <= num_pages {
        return -1;
    }
    drop(frame_allocator);

    let start_addr = VirtAddr::from(start);
    let start_pagenum = VirtPageNum::from(start_addr);
    let end = start + len;
    let end_addr = VirtAddr::from(end);
    //let end_pagenum = end_addr.ceil();

    let vir_memory_set = &mut inner.memory_set;
    for i in (start_pagenum.0)..(start_pagenum.0 + num_pages){
        let pte = vir_memory_set.page_table.translate(VirtPageNum::from(i));
        match pte{
            Some(pte) => {
                if pte.is_valid(){
                    return -1;
                }
            },
            None => {},
        }
    }

    let mut permissions = MapPermission::U;
    if port / 4 == 1{
        permissions = permissions | MapPermission::X;
    }
    if port % 2 == 1{
        permissions = permissions | MapPermission::R;
    }
    if (port % 4) / 2 == 1{
        permissions = permissions | MapPermission::W;
    }
    
    vir_memory_set.insert_framed_area(start_addr,end_addr,permissions);

    drop(inner);
    drop(current);
    0
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap",
        current_task().unwrap().pid.0
    );
    let current = current_task().unwrap();
    let mut inner = current.inner_exclusive_access();
    inner.syscall_times[8] += 1;

    if start & (PAGE_SIZE-1) != 0 {
        return -1;
    }
    
    let num_pages = (len + PAGE_SIZE - 1) / PAGE_SIZE;

    let start_addr = VirtAddr::from(start);
    let start_pagenum = VirtPageNum::from(start_addr);

    let vir_memory_set = &mut inner.memory_set;
    for n in 0..num_pages{
        let pte = vir_memory_set.page_table.find_pte(VirtPageNum::from(start_pagenum.0 + n));
        match pte{
            Some(_x) => {},
            None => {return -1;},
        }
    }  

    let memory_set = &mut inner.memory_set;
    let areas = &mut memory_set.areas;
    let page_table = &mut memory_set.page_table;
    let mut ok:bool = false;
    for i in 0..areas.len(){
        let range = areas[i].vpn_range;
        if range.l.0 == start_pagenum.0 && range.r.0 == start_pagenum.0 + num_pages{
            areas[i].unmap(page_table);
            areas.remove(i);
            ok = true;
            break;
        }
    }
    if ok == false{
        return -1;
    }
    drop(inner);
    drop(current);
    0
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    let current = current_task().unwrap();
    let mut inner = current.inner_exclusive_access();
    inner.syscall_times[7] += 1;
    drop(inner);
    drop(current);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn",
        current_task().unwrap().pid.0
    );

    let current_task = current_task().unwrap();
    let mut inner = current_task.inner_exclusive_access();
    inner.syscall_times[13] += 1;
    drop(inner);
    let token = current_user_token();
    let path = translated_str(token, path);

    let Some(data) = get_app_data_by_name(path.as_str()) else { return -1; };
    let new_task_control_block = Arc::new(TaskControlBlock::new(&data));
    let new_pid = new_task_control_block.getpid();
    let mut new_task_inner = new_task_control_block.inner_exclusive_access();

    new_task_inner.parent = Some(Arc::downgrade(&current_task));
    drop(new_task_inner);
    add_task(new_task_control_block.clone());
    
    let mut inner = current_task.inner_exclusive_access();
    inner.children.push(new_task_control_block.clone());
    drop(inner);

    drop(current_task);
    new_pid as isize

}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority",
        current_task().unwrap().pid.0
    );
    let current = current_task().unwrap();
    let mut inner = current.inner_exclusive_access();
    inner.syscall_times[4] += 1;

    if prio <= 1{ return -1;}
    
    inner.priority = prio;
    drop(inner);
    drop(current);
    prio
}
