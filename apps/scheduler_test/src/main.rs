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
use trap_handler::CurrentTimebaseFrequency;

struct CurrentTimebaseFrequencyImpl;

#[crate_interface::impl_interface]
impl CurrentTimebaseFrequency for CurrentTimebaseFrequencyImpl {
    // 获取dtb上的时基频率比较困难，因此乱填的
    fn current_timebase_frequency() -> usize {
        1000_000
    }
}

#[no_mangle]
fn main(cpu_id: usize, cpu_num: usize) {
    trap_handler::init_main_processor();
    task_management::init_main_processor(test_block_wake_yield, cpu_id, cpu_num);
}

#[no_mangle]
fn main_secondary(cpu_id: usize) {
    trap_handler::init_secondary_processor();
    task_management::init_secondary_processor(cpu_id);
}

static BLOCK_QUEUE: LazyInit<SpinNoIrq<BlockQueue>> = LazyInit::new();

fn test_block_wake_yield() -> i32 {

    // println!("interrupt enable status:");
    // println!("    sstatus.sie: {}", sstatus::read().sie());
    // println!("    sie.sext: {}", sie::read().sext());
    // println!("    sie.ssoft: {}", sie::read().ssoft());
    // println!("    sie.stimer: {}", sie::read().stimer());

    // unsafe { asm!("ebreak"); }

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