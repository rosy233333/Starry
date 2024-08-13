#![no_std]
#![no_main]

use core::arch::asm;

use axlog::warn;
use lazy_init::LazyInit;
use riscv::register::{sie, sstatus};
use spinlock::SpinNoIrq;
use task_management::*;
use axstd::println;

extern crate alloc;
use alloc::sync::Arc;
use trap_handler::{disable_irqs, enable_irqs, CurrentTimebaseFrequency};

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
    start_main_processor(test_preempt);
    // start_main_processor(test_block_wake_yield);
}

#[no_mangle]
fn main_secondary(cpu_id: usize) {
    disable_irqs();
    task_management::init_secondary_processor(cpu_id);
    trap_handler::init_secondary_processor();
    start_secondary_processor();
}

static BLOCK_QUEUE: LazyInit<SpinNoIrq<BlockQueue>> = LazyInit::new();

fn test_block_wake_yield() -> i32 {

    BLOCK_QUEUE.init_by(SpinNoIrq::new(BlockQueue::new()));
    for i in 0 .. 2 {
        spawn_to_global(|| {
            let task_id = current_id();
            warn!("task {task_id} is thread");
            let cpu_id_1 = current_processor_id();
            warn!("before yield, thread {task_id} is running on cpu {cpu_id_1}");
    
            yield_current_to_local();

            let cpu_id_2 = current_processor_id();
            warn!("after yield, before block, thread {task_id} is running on cpu {cpu_id_2}");

            BlockQueue::block_current_with_locked_self(&*BLOCK_QUEUE, SpinNoIrq::lock);

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

            BlockQueue::block_current_async_with_locked_self(&*BLOCK_QUEUE, SpinNoIrq::lock).await;
            warn!("async task can block with sync method!");
            BlockQueue::block_current_with_locked_self(&*BLOCK_QUEUE, SpinNoIrq::lock);

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

fn test_interrupt() -> i32{
    println!("interrupt enable status:");
    println!("    sstatus.sie: {}", sstatus::read().sie());
    println!("    sie.sext: {}", sie::read().sext());
    println!("    sie.ssoft: {}", sie::read().ssoft());
    println!("    sie.stimer: {}", sie::read().stimer());

    unsafe { asm!("ebreak"); }
    0
}

fn test_preempt() -> i32{
    enable_irqs();
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