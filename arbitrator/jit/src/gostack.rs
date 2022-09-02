// Copyright 2022, Offchain Labs, Inc.
// For license information, see https://github.com/nitro/blob/master/LICENSE

use crate::syscall::{DynamicObjectPool, JsValue, PendingEvent};

use parking_lot::{Mutex, MutexGuard};
use rand_pcg::Pcg32;
use wasmer::{Memory, MemoryView, WasmPtr, WasmerEnv};

use std::{
    collections::{BTreeSet, BinaryHeap, VecDeque},
    ops::Deref,
    sync::Arc,
};

#[derive(Clone)]
pub struct GoStack {
    start: u32,
    memory: Memory,
}

#[allow(dead_code)]
impl GoStack {
    pub fn new_sans_env(start: u32, env: &WasmEnvArc) -> Self {
        let memory = env.lock().memory.clone().unwrap();
        Self { start, memory }
    }

    pub fn new(start: u32, env: &WasmEnvArc) -> (Self, MutexGuard<WasmEnv>) {
        let sp = GoStack::new_sans_env(start, env);
        let env = env.lock();
        (sp, env)
    }

    fn offset(&self, arg: u32) -> u32 {
        self.start + (arg + 1) * 8
    }

    pub fn read_u8(&self, arg: u32) -> u8 {
        self.read_u8_ptr(self.offset(arg))
    }

    pub fn read_u32(&self, arg: u32) -> u32 {
        self.read_u32_ptr(self.offset(arg))
    }

    pub fn read_u64(&self, arg: u32) -> u64 {
        self.read_u64_ptr(self.offset(arg))
    }

    pub fn read_u8_ptr(&self, ptr: u32) -> u8 {
        let ptr: WasmPtr<u8> = WasmPtr::new(ptr);
        ptr.deref(&self.memory).unwrap().get()
    }

    pub fn read_u32_ptr(&self, ptr: u32) -> u32 {
        let ptr: WasmPtr<u32> = WasmPtr::new(ptr);
        ptr.deref(&self.memory).unwrap().get()
    }

    pub fn read_u64_ptr(&self, ptr: u32) -> u64 {
        let ptr: WasmPtr<u64> = WasmPtr::new(ptr);
        ptr.deref(&self.memory).unwrap().get()
    }

    pub fn write_u8(&self, arg: u32, x: u8) {
        self.write_u8_ptr(self.offset(arg), x);
    }

    pub fn write_u32(&self, arg: u32, x: u32) {
        self.write_u32_ptr(self.offset(arg), x);
    }

    pub fn write_u64(&self, arg: u32, x: u64) {
        self.write_u64_ptr(self.offset(arg), x);
    }

    pub fn write_u8_ptr(&self, ptr: u32, x: u8) {
        let ptr: WasmPtr<u8> = WasmPtr::new(ptr);
        ptr.deref(&self.memory).unwrap().set(x);
    }

    pub fn write_u32_ptr(&self, ptr: u32, x: u32) {
        let ptr: WasmPtr<u32> = WasmPtr::new(ptr);
        ptr.deref(&self.memory).unwrap().set(x);
    }

    pub fn write_u64_ptr(&self, ptr: u32, x: u64) {
        let ptr: WasmPtr<u64> = WasmPtr::new(ptr);
        ptr.deref(&self.memory).unwrap().set(x);
    }

    pub fn read_slice(&self, ptr: u64, len: u64) -> Vec<u8> {
        let ptr = u32::try_from(ptr).expect("Go pointer not a u32") as usize;
        let len = u32::try_from(len).expect("length isn't a u32") as usize;
        unsafe { self.memory.data_unchecked()[ptr..ptr + len].to_vec() }
    }

    pub fn write_slice(&self, ptr: u64, src: &[u8]) {
        let ptr = u32::try_from(ptr).expect("Go pointer not a u32");
        let view: MemoryView<u8> = self.memory.view();
        let view = view.subarray(ptr, ptr + src.len() as u32);
        unsafe { view.copy_from(src) }
    }

    pub fn read_value_slice(&self, mut ptr: u64, len: u64) -> Vec<JsValue> {
        let mut values = Vec::new();
        for _ in 0..len {
            let p = u32::try_from(ptr).expect("Go pointer not a u32");
            values.push(JsValue::new(self.read_u64_ptr(p)));
            ptr += 8;
        }
        values
    }
}

#[derive(Clone, WasmerEnv)]
pub struct WasmEnv {
    /// Mechanism for reading and writing the module's memory
    pub memory: Option<Memory>,
    /// An increasing clock used when Go asks for time, measured in nanoseconds
    pub time: u64,
    /// The amount of time advanced each check. Currently 10 milliseconds
    pub time_interval: u64,
    /// The state of Go's timeouts
    pub timeouts: TimeoutState,
    /// Deterministic source of random data
    pub rng: Pcg32,
    /// A collection of js objects
    pub js_object_pool: DynamicObjectPool,
    /// The event Go will execute next
    pub js_pending_event: Option<PendingEvent>,
    /// Future events that Go has scheduled after the next up
    pub js_future_events: VecDeque<PendingEvent>,
}

impl Default for WasmEnv {
    fn default() -> Self {
        Self {
            memory: None,
            time: 0,
            time_interval: 10_000_000,
            timeouts: TimeoutState::default(),
            rng: Pcg32::new(0xcafef00dd15ea5e5, 0xa02bdbf7bb3c0a7),
            js_object_pool: DynamicObjectPool::default(),
            js_pending_event: None,
            js_future_events: VecDeque::new(),
        }
    }
}

#[derive(Clone, Default, WasmerEnv)]
pub struct WasmEnvArc(Arc<Mutex<WasmEnv>>);

impl Deref for WasmEnvArc {
    type Target = Mutex<WasmEnv>;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeoutInfo {
    pub time: u64,
    pub id: u32,
}

impl Ord for TimeoutInfo {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .time
            .cmp(&self.time)
            .then_with(|| other.id.cmp(&self.id))
    }
}

impl PartialOrd for TimeoutInfo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(&other))
    }
}

#[derive(Default, Clone, Debug)]
pub struct TimeoutState {
    /// Contains tuples of (time, id)
    pub times: BinaryHeap<TimeoutInfo>,
    pub pending_ids: BTreeSet<u32>,
    pub next_id: u32,
}
