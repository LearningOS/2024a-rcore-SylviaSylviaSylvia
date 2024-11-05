//! File and filesystem-related syscalls
use easy_fs::{layout::DIRENT_SZ, block_cache::{block_cache_sync_all,get_block_cache}, layout::{DirEntry,DiskInode},};
use crate::fs::{open_file, OpenFlags, Stat, inode::ROOT_INODE};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer, VirtPageNum,VirtAddr,PhysAddr};
use crate::task::{current_task, current_user_token};
use alloc::sync::Arc;
use alloc::vec::Vec;

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(fd: usize, st: *mut Stat) -> isize {
    trace!(
        "kernel:pid[{}] sys_fstat",
        current_task().unwrap().pid.0
    );
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }

    let st_addr = VirtAddr::from(st as usize);
    let st_page_num = VirtPageNum::from(st_addr.floor());
    let st_offset = st_addr.page_offset();

    let phys_page_num = inner.memory_set.translate(st_page_num).unwrap().ppn();
    
    let phys_addr = PhysAddr::from(phys_page_num);
    let phys_st = (phys_addr.0 + st_offset) as *mut Stat;

    let Some(ref stat) = inner.fd_table[fd] else { return -1; };

    unsafe{
        *phys_st = stat.get_stat();
    }
    drop(inner);
    drop(task);
    0
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(old_name: *const u8, new_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_linkat",
        current_task().unwrap().pid.0
    );
    if old_name == new_name{
        return -1;
    }
    let token = current_user_token();
    let old_name = translated_str(token, old_name);
    let new_name = translated_str(token, new_name);

    let mut fs = ROOT_INODE.fs.lock();
    let Some(inode_id) = ROOT_INODE.read_disk_inode(|disk_inode| {
        ROOT_INODE.find_inode_id(&old_name.as_str(), disk_inode)
    }) else { return -1; };

    let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
    get_block_cache(block_id as usize, Arc::clone(&ROOT_INODE.block_device))
        .lock()
        .modify(block_offset, |disk_inode: &mut DiskInode| disk_inode.nlink_num += 1);

    ROOT_INODE.modify_disk_inode(|root_inode| {
        let file_count = (root_inode.size as usize) / DIRENT_SZ;
        let new_size = (file_count + 1) * DIRENT_SZ;
        ROOT_INODE.increase_size(new_size as u32, root_inode, &mut fs);
        
        let dirent = DirEntry::new(&new_name.as_str(), inode_id);
        
        root_inode.write_at(
            file_count * DIRENT_SZ,
             dirent.as_bytes(), 
             &ROOT_INODE.block_device
        );
    });

    block_cache_sync_all();  
    0
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let name = translated_str(token,name);

    let mut fs = ROOT_INODE.fs.lock();
    let mut v: Vec<DirEntry> = Vec::new();
    let mut inode_id = 0;

    ROOT_INODE.modify_disk_inode(|root_inode| {
        let file_count = (root_inode.size as usize) / DIRENT_SZ;
        for i in 0..file_count {
            let mut dirent = DirEntry::empty();
            assert_eq!(
                root_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &ROOT_INODE.block_device,),
                DIRENT_SZ,
            );
            if dirent.name() != name {
                v.push(dirent);
            } else {
                inode_id = dirent.inode_id();
            }
        }
    });
    ROOT_INODE.modify_disk_inode(|root_inode| {
        let size = root_inode.size;
        let data_blocks_dealloc = root_inode.clear_size(&ROOT_INODE.block_device);
        assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
        for data_block in data_blocks_dealloc.into_iter() {
            fs.dealloc_data(data_block);
        }
        ROOT_INODE.increase_size((v.len() * DIRENT_SZ) as u32, root_inode, &mut fs);
        for (i, dirent) in v.iter().enumerate() {
            root_inode.write_at(i * DIRENT_SZ, dirent.as_bytes(), &ROOT_INODE.block_device);
        }
    });

    let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);

    get_block_cache(block_id as usize, Arc::clone(&ROOT_INODE.block_device))
        .lock()
        .modify(block_offset, |disk_inode: &mut DiskInode| {
            disk_inode.nlink_num -= 1;
            if disk_inode.nlink_num == 0 {
                let size = disk_inode.size;
                let data_blocks_dealloc = disk_inode.clear_size(&ROOT_INODE.block_device);
                assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
                for data_block in data_blocks_dealloc.into_iter() {
                    fs.dealloc_data(data_block);
                }
            }
        });

    block_cache_sync_all();
    0
}
