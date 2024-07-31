#![no_std]
#![no_main]

use axlog::warn;
use lazy_init::LazyInit;
use spinlock::SpinNoIrq;
use task_management::*;
use axstd::println;

extern crate alloc;
use alloc::sync::Arc;

#[no_mangle]
fn main(cpu_id: usize, cpu_num: usize) {
    init_main_processor(test_block_wake_yield, cpu_id, cpu_num);
}

#[no_mangle]
fn main_secondary(cpu_id: usize) {
    init_secondary_processor(cpu_id);
}

static BLOCK_QUEUE: LazyInit<SpinNoIrq<BlockQueue>> = LazyInit::new();

fn test_block_wake_yield() -> i32 {
    BLOCK_QUEUE.init_by(SpinNoIrq::new(BlockQueue::new()));
    for i in 0 .. 4 {
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

    for i in 0 .. 4 {
        spawn_to_global_async(async {
            let task_id = current_id();
            warn!("task {task_id} is coroutine");
            let cpu_id_1 = current_processor_id();
            warn!("before yield, coroutine {task_id} is running on cpu {cpu_id_1}");
    
            yield_current_to_local_async().await;

            let cpu_id_2 = current_processor_id();
            warn!("after yield, before block, coroutine {task_id} is running on cpu {cpu_id_2}");

            BlockQueue::block_current_async_with_locked_self(&*BLOCK_QUEUE, SpinNoIrq::lock).await;

            let cpu_id_3 = current_processor_id();
            warn!("after wake, coroutine {task_id} is running on cpu {cpu_id_3}");

            exit_current_async(0);
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