//! Process management syscalls
use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE},
    task::{
        change_program_brk, exit_current_and_run_next, suspend_current_and_run_next, 
        TaskStatus,TASK_MANAGER
    },
    timer::get_time_us,
    mm::{VirtAddr,VirtPageNum,PhysAddr,MapPermission,frame_allocator::*,PTEFlags}
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
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
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

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");

    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    inner.tasks[current].syscall_times[3] += 1;
    drop(inner);
    let us = get_time_us();
    let ts_addr = VirtAddr::from(ts as usize);
    let ts_page_num = VirtPageNum::from(ts_addr.floor());
    let ts_offset = ts_addr.page_offset();
    let inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    let phys_page_num = inner.tasks[current].memory_set.translate(ts_page_num).unwrap().ppn();
    
    let phys_addr = PhysAddr::from(phys_page_num);
    let phys_ts = (phys_addr.0 + ts_offset) as *mut TimeVal;

    unsafe {
        *(phys_ts) = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    drop(inner);
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info");
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    inner.tasks[current].syscall_times[7] += 1;
    
    let start_time = inner.tasks[current].start_time;
    let current_time = get_time_us() / 1000;

    let ti_addr = VirtAddr::from(ti as usize);
    let ti_page_num = VirtPageNum::from(ti_addr.floor());
    let ti_offset = ti_addr.page_offset();
    let phys_page_num = inner.tasks[current].memory_set.translate(ti_page_num).unwrap().ppn();
    let phys_addr = PhysAddr::from(phys_page_num);
    let phys_ti = (phys_addr.0 + ti_offset) as *mut TaskInfo;

    unsafe {
        for i in 0..MAX_SYSCALL_NUM {
            if i != 64 && i != 93 && i != 124 && i !=214 && i != 215 && i != 222 && i != 169 && i != 410 {
                (*(phys_ti)).syscall_times[i] = 0;
            }
        }

        (*(phys_ti)).syscall_times[64] = inner.tasks[current].syscall_times[0];
        (*(phys_ti)).syscall_times[93] = inner.tasks[current].syscall_times[1];
        (*(phys_ti)).syscall_times[124] = inner.tasks[current].syscall_times[2];
        (*(phys_ti)).syscall_times[169] = inner.tasks[current].syscall_times[3];
        (*(phys_ti)).syscall_times[214] = inner.tasks[current].syscall_times[4];
        (*(phys_ti)).syscall_times[215] = inner.tasks[current].syscall_times[5];
        (*(phys_ti)).syscall_times[222] = inner.tasks[current].syscall_times[6];
        (*(phys_ti)).syscall_times[410] = inner.tasks[current].syscall_times[7];
        (*(phys_ti)).time = current_time - start_time;
    }

    drop(inner);
    0
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!("kernel: sys_mmap");
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    inner.tasks[current].syscall_times[6] += 1;
    
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
    let end_pagenum = VirtPageNum::from(end_addr.ceil());

    let vir_memory_set = &mut inner.tasks[current].memory_set;
    for i in (start_pagenum.0)..(end_pagenum.0){
        let pte = vir_memory_set.page_table.find_pte(VirtPageNum::from(i));
        match pte{
            Some(_x) => {return -1;},
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

    let mut flags = PTEFlags::V | PTEFlags::U;
    if port / 4 == 1{
        flags = flags | PTEFlags::X;
    }
    if port % 2 == 1{
        flags = flags | PTEFlags::R;
    }
    if (port % 4) / 2 == 1{
        flags = flags | PTEFlags::W;
    }

    for i in 0..num_pages{
        let frame = frame_alloc().unwrap();
        vir_memory_set.page_table.map(VirtPageNum::from(start_pagenum.0 + i),frame.ppn,flags);
    }
    vir_memory_set.insert_framed_area(start_addr,end_addr,permissions);

    drop(inner);
    0
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    inner.tasks[current].syscall_times[5] += 1;

    if start & (PAGE_SIZE-1) != 0 {
        return -1;
    }
    
    let num_pages = (len + PAGE_SIZE - 1) / PAGE_SIZE;

    let start_addr = VirtAddr::from(start);
    let start_pagenum = VirtPageNum::from(start_addr);

    let vir_memory_set = &mut inner.tasks[current].memory_set;
    for n in 0..num_pages{
        let pte = vir_memory_set.page_table.find_pte(VirtPageNum::from(start_pagenum.0 + n));
        match pte{
            Some(_x) => {},
            None => {return -1;},
        }
    }

    for i in 0..num_pages{
        vir_memory_set.page_table.unmap(VirtPageNum::from(start_pagenum.0 + i));
    }
    drop(inner);

    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    let areas = &mut inner.tasks[current].memory_set.areas;
    for i in 0..areas.len(){
        let range = areas[i].vpn_range;
        if range.l.0 == start_pagenum.0 {
            areas.pop();
            break;
        }
    }
    drop(inner);
    0
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    inner.tasks[current].syscall_times[4] += 1;
    drop(inner);

    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
