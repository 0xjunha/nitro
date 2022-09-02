// Copyright 2022, Offchain Labs, Inc.
// For license information, see https://github.com/nitro/blob/master/LICENSE

use crate::gostack::{GoStack, TimeoutInfo, WasmEnvArc};

use rand::RngCore;
use thiserror::Error;

use std::io::Write;

#[derive(Error, Debug)]
pub enum Escape {
    #[error("program exited with status code `{0}`")]
    Exit(u32),
    #[error("jit failed with `{0}`")]
    Failure(String),
}

pub fn go_debug(x: u32) {
    println!("go debug: {x}")
}

pub fn reset_memory_data_view(_: u32) {}

pub fn wasm_exit(env: &WasmEnvArc, sp: u32) -> Result<(), Escape> {
    let sp = GoStack::new_sans_env(sp, env);
    Err(Escape::Exit(sp.read_u32(0)))
}

pub fn wasm_write(env: &WasmEnvArc, sp: u32) {
    let sp = GoStack::new_sans_env(sp, env);
    let fd = sp.read_u64(0);
    let ptr = sp.read_u64(1);
    let len = sp.read_u32(2);
    let buf = sp.read_slice(ptr, len.into());
    if fd == 2 {
        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();
        stderr.write_all(&buf).unwrap();
    } else {
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        stdout.write_all(&buf).unwrap();
    }
}

pub fn nanotime1(env: &WasmEnvArc, sp: u32) {
    let (sp, mut env) = GoStack::new(sp, env);
    env.time += env.time_interval;
    sp.write_u64(0, env.time);
}

pub fn walltime(env: &WasmEnvArc, sp: u32) {
    let (sp, mut env) = GoStack::new(sp, env);
    env.time += env.time_interval;
    sp.write_u64(0, env.time / 1_000_000_000);
    sp.write_u32(1, (env.time % 1_000_000_000) as u32);
}

pub fn walltime1(env: &WasmEnvArc, sp: u32) {
    let (sp, mut env) = GoStack::new(sp, env);
    env.time += env.time_interval;
    sp.write_u64(0, env.time / 1_000_000_000);
    sp.write_u64(1, env.time % 1_000_000_000);
}

pub fn schedule_timeout_event(env: &WasmEnvArc, sp: u32) {
    let (sp, mut env) = GoStack::new(sp, env);
    let mut time = sp.read_u64(0);
    time = time.saturating_mul(1_000_000); // milliseconds to nanoseconds
    time = time.saturating_add(env.time); // add the current time to the delay

    let timeouts = &mut env.timeouts;
    let id = timeouts.next_id;
    timeouts.next_id += 1;
    timeouts.times.push(TimeoutInfo { time, id });
    timeouts.pending_ids.insert(id);

    sp.write_u32(1, id);
}

pub fn clear_timeout_event(env: &WasmEnvArc, sp: u32) {
    let (sp, mut env) = GoStack::new(sp, env);

    let id = sp.read_u32(0);
    if !env.timeouts.pending_ids.remove(&id) {
        eprintln!("Go attempting to clear not pending timeout event {id}");
    }
}

pub fn get_random_data(env: &WasmEnvArc, sp: u32) {
    let (sp, mut env) = GoStack::new(sp, env);

    let mut ptr = u32::try_from(sp.read_u64(0)).expect("Go getRandomData pointer not a u32");
    let mut len = sp.read_u64(1);
    while len >= 4 {
        sp.write_u32_ptr(ptr, env.rng.next_u32());
        ptr += 4;
        len -= 4;
    }
    if len > 0 {
        let mut rem = env.rng.next_u32();
        for _ in 0..len {
            sp.write_u8_ptr(ptr, rem as u8);
            ptr += 1;
            rem >>= 8;
        }
    }
}
