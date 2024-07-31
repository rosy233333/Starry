#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]

#[cfg(feature = "axstd")]
use axstd::println;

#[cfg_attr(feature = "axstd", no_mangle)]
fn main(cpu_id: usize, cpu_num: usize) {
    println!("Hello, world from main cpu {cpu_id} and all {cpu_num} cpus!");
}

#[cfg_attr(feature = "axstd", no_mangle)]
fn main_secondary(cpu_id: usize) {
    println!("Hello, world from secondary cpu {cpu_id}!");
}