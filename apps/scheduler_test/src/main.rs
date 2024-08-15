#![no_std]
#![no_main]

use core::{arch::asm, sync::atomic::{AtomicUsize, Ordering}};

use axlog::{info, warn};
use lazy_init::LazyInit;
use riscv::register::{scause::{Exception, Trap}, sie, sstatus};
use spinlock::{SpinNoIrq, SpinNoIrqOnly};
use task_management::*;
use axstd::{println, thread::yield_now, time::{self, Instant}};

extern crate alloc;
use alloc::sync::Arc;
use trap_handler::{disable_irqs, enable_irqs, register_trap_handler, CurrentTimebaseFrequency};

struct CurrentTimebaseFrequencyImpl;

#[crate_interface::impl_interface]
impl CurrentTimebaseFrequency for CurrentTimebaseFrequencyImpl {
    // 获取dtb上的时基频率比较困难，因此乱填的
    fn current_timebase_frequency() -> usize {
        1_000_000_000
    }
}

#[no_mangle]
fn main(cpu_id: usize, cpu_num: usize) {
    disable_irqs();
    task_management::init_main_processor(cpu_id, cpu_num);
    trap_handler::init_main_processor();
    start_main_processor(test_block_wake_yield);
    // start_main_processor(test_trap);
    // start_main_processor(test_preempt);
    // start_main_processor(test_yield_performance);
    // start_main_processor(test_yield_performance_async);
    // start_main_processor(test_block_performance);
    // start_main_processor(test_block_performance_async);
}

#[no_mangle]
fn main_secondary(cpu_id: usize) {
    disable_irqs();
    task_management::init_secondary_processor(cpu_id);
    trap_handler::init_secondary_processor();
    start_secondary_processor();
}

static BLOCK_QUEUE: LazyInit<SpinNoIrqOnly<BlockQueue>> = LazyInit::new();

fn test_block_wake_yield() -> i32 {

    BLOCK_QUEUE.init_by(SpinNoIrqOnly::new(BlockQueue::new()));
    for i in 0 .. 2 {
        spawn_to_global(|| {
            let task_id = current_id();
            warn!("task {task_id} is thread");
            let cpu_id_1 = current_processor_id();
            warn!("before yield, thread {task_id} is running on cpu {cpu_id_1}");
    
            yield_current_to_local();

            let cpu_id_2 = current_processor_id();
            warn!("after yield, before block, thread {task_id} is running on cpu {cpu_id_2}");

            BlockQueue::block_current_with_locked_self(&*BLOCK_QUEUE, SpinNoIrqOnly::lock);

            let cpu_id_3 = current_processor_id();
            warn!("after wake, thread {task_id} is running on cpu {cpu_id_3}");

            exit_current(0);
            -1
        });
    }

    for i in 0 .. 2 {
        spawn_to_global_async(async {
            let task_id = current_id();
            warn!("task {task_id} is coroutine");
            let cpu_id_1 = current_processor_id();
            warn!("before yield, coroutine {task_id} is running on cpu {cpu_id_1}");
    
            yield_current_to_local_async().await;
            warn!("async task can yield with sync method!");
            yield_current_to_local();

            let cpu_id_2 = current_processor_id();
            warn!("after yield, before block, coroutine {task_id} is running on cpu {cpu_id_2}");

            BlockQueue::block_current_async_with_locked_self(&*BLOCK_QUEUE, SpinNoIrqOnly::lock).await;
            warn!("async task can block with sync method!");
            BlockQueue::block_current_with_locked_self(&*BLOCK_QUEUE, SpinNoIrqOnly::lock);

            let cpu_id_3 = current_processor_id();
            warn!("after wake, coroutine {task_id} is running on cpu {cpu_id_3}");

            exit_current_async(0).await;
            -1
        });
    }

    loop {
        BLOCK_QUEUE.lock().wake_all_to_global();
    }

    let task_id = current_id();
    warn!("main task {task_id}: task spawn complete");
    0
}

fn test_trap() -> i32{
    register_trap_handler(Trap::Exception(Exception::Breakpoint), |_stval, task_context| {
        task_context.step_sepc(); // 使保存的sepc前进一条指令
        warn!("handle breakpoint exception!");
    });

    unsafe { asm!(
        "ebreak", // 触发Breakpoint异常（0x3）
        "
        li      t0, 0
        addi    t0, t0, -1
        sw      a0, 0(t0)
        ", // 触发Store page fault异常（0xf）
    ); }
    0
}

fn test_preempt() -> i32{
    // main任务的初始优先级为1
    assert!(spawn_to_local_with_priority(|| {
        println!("Main task is preempted by sync task!");
        assert!(change_current_priority(2).is_ok());
        loop { }
    }, 0).is_ok());
    // println!("Sync task is preempted by main task!");
    assert!(spawn_to_local_async_with_priority(async {
        println!("Main task is preempted by async task!");
        assert!(change_current_priority(2).is_ok());
        loop { }
    }, 0).is_ok());
    // println!("Async task is preempted by main task!");
    println!("task spawn complete!");
    loop { }
    0
}

static TASK_NUM: usize = 100;
static YIELD_PER_TASK: usize = 100;
static START_TASKS: AtomicUsize = AtomicUsize::new(0);
static END_TASKS: AtomicUsize = AtomicUsize::new(0);
// static TIME_BASE: LazyInit<Instant> = LazyInit::new();
static START_TIME: LazyInit<Instant> = LazyInit::new();

fn test_yield_performance() -> i32 {
    // let first_start_time = time::Instant::now();
    // TIME_BASE.init_by(time::Instant::now());
    for _ in 0 .. TASK_NUM {
        spawn_to_global(|| {
            let start_time = time::Instant::now();
            if START_TASKS.fetch_add(1, Ordering::AcqRel) == 0 {
                // 如果是第一个开始的任务
                // warn!("first start time: {} μs", start_time.duration_since(*TIME_BASE).as_micros());
                START_TIME.init_by(start_time);
            }
            for _ in 0 .. YIELD_PER_TASK {
                yield_current_to_local();
            }
            let end_time = time::Instant::now();
            // info!("one task finish after {} μs", end_time.duration_since(start_time).as_micros());
            info!("{}", end_time.duration_since(start_time).as_micros());
            if END_TASKS.fetch_add(1, Ordering::AcqRel) == TASK_NUM - 1 {
                // 如果是最后一个完成的任务
                // warn!("last end time: {} μs", end_time.duration_since(*TIME_BASE).as_micros());
                // warn!("all tasks finish after {} μs", end_time.duration_since(time_base).as_micros());
                warn!("{}", end_time.duration_since(*START_TIME).as_micros());
            }
            0
        });
    }
    loop {
        yield_now();
    }
    0
}


fn test_yield_performance_async() -> i32 {
    // let first_start_time = time::Instant::now();
    // TIME_BASE.init_by(time::Instant::now());
    for _ in 0 .. TASK_NUM {
        spawn_to_global_async(async {
            let start_time = time::Instant::now();
            if START_TASKS.fetch_add(1, Ordering::AcqRel) == 0 {
                // 如果是第一个开始的任务
                // warn!("first start time: {} μs", start_time.duration_since(*TIME_BASE).as_micros());
                START_TIME.init_by(start_time);
            }
            for _ in 0 .. YIELD_PER_TASK {
                yield_current_to_local_async().await;
            }
            let end_time = time::Instant::now();
            // info!("one task finish after {} μs", end_time.duration_since(start_time).as_micros());
            info!("{}", end_time.duration_since(start_time).as_micros());
            if END_TASKS.fetch_add(1, Ordering::AcqRel) == TASK_NUM - 1 {
                // 如果是最后一个完成的任务
                // warn!("last end time: {} μs", end_time.duration_since(*TIME_BASE).as_micros());
                // warn!("all tasks finish after {} μs", end_time.duration_since(time_base).as_micros());
                warn!("{}", end_time.duration_since(*START_TIME).as_micros());
            }
            0
        });
    }
    loop {
        yield_now();
    }
    0
}

static BLOCK_PER_TASK: usize = 100;

fn test_block_performance() -> i32 {
    BLOCK_QUEUE.init_by(SpinNoIrqOnly::new(BlockQueue::new()));
    // let first_start_time = time::Instant::now();
    // TIME_BASE.init_by(time::Instant::now());
    for _ in 0 .. TASK_NUM {
        spawn_to_global(|| {
            let start_time = time::Instant::now();
            if START_TASKS.fetch_add(1, Ordering::AcqRel) == 0 {
                // 如果是第一个开始的任务
                // warn!("first start time: {} μs", start_time.duration_since(*TIME_BASE).as_micros());
                START_TIME.init_by(start_time);
            }
            for _ in 0 .. BLOCK_PER_TASK {
                BlockQueue::block_current_with_locked_self(&*BLOCK_QUEUE, SpinNoIrqOnly::lock);
            }
            let end_time = time::Instant::now();
            // info!("one task finish after {} μs", end_time.duration_since(start_time).as_micros());
            info!("{}", end_time.duration_since(start_time).as_micros());
            if END_TASKS.fetch_add(1, Ordering::AcqRel) == TASK_NUM - 1 {
                // 如果是最后一个完成的任务
                // warn!("last end time: {} μs", end_time.duration_since(*TIME_BASE).as_micros());
                // warn!("all tasks finish after {} μs", end_time.duration_since(time_base).as_micros());
                warn!("{}", end_time.duration_since(*START_TIME).as_micros());
            }
            0
        });
    }

    loop {
        // info!("waking {} tasks", BLOCK_QUEUE.lock().wake_all_to_global());
        BLOCK_QUEUE.lock().wake_all_to_global();
    }
    0
}

fn test_block_performance_async() -> i32 {
    BLOCK_QUEUE.init_by(SpinNoIrqOnly::new(BlockQueue::new()));
    // let first_start_time = time::Instant::now();
    // TIME_BASE.init_by(time::Instant::now());
    for _ in 0 .. TASK_NUM {
        spawn_to_global_async(async {
            let start_time = time::Instant::now();
            if START_TASKS.fetch_add(1, Ordering::AcqRel) == 0 {
                // 如果是第一个开始的任务
                // warn!("first start time: {} μs", start_time.duration_since(*TIME_BASE).as_micros());
                START_TIME.init_by(start_time);
            }
            for _ in 0 .. BLOCK_PER_TASK {
                BlockQueue::block_current_async_with_locked_self(&*BLOCK_QUEUE, SpinNoIrqOnly::lock).await;
            }
            let end_time = time::Instant::now();
            // info!("one task finish after {} μs", end_time.duration_since(start_time).as_micros());
            info!("{}", end_time.duration_since(start_time).as_micros());
            if END_TASKS.fetch_add(1, Ordering::AcqRel) == TASK_NUM - 1 {
                // 如果是最后一个完成的任务
                // warn!("last end time: {} μs", end_time.duration_since(*TIME_BASE).as_micros());
                // warn!("all tasks finish after {} μs", end_time.duration_since(time_base).as_micros());
                warn!("{}", end_time.duration_since(*START_TIME).as_micros());
            }
            0
        });
    }

    loop {
        // info!("waking {} tasks", BLOCK_QUEUE.lock().wake_all_to_global());
        BLOCK_QUEUE.lock().wake_all_to_global();
    }
    0
}