// SPDX-License-Identifier: GPL-3.0-or-later

//! Original guest fixture services for exercising manual task dispatch.

use super::{ERROR_INVALID_PARAMETER, ERROR_NOACCESS, ERROR_NOT_ENOUGH_MEMORY, ServicesState};
use crate::imports::ImportRegistry;
use crate::tasks::{TaskId, TaskQueue, TaskResult};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MODULE: &str = "xenora_fixture.dll";
const COMPLETION_SIZE: usize = 24;

type Delivered = Option<(TaskId, TaskResult<u64, u32>)>;

pub(super) struct TaskFixture {
    queue: TaskQueue<u64, u32>,
    delivered: Arc<Mutex<Delivered>>,
}

impl Default for TaskFixture {
    fn default() -> Self {
        Self {
            queue: TaskQueue::new(16),
            delivered: Arc::default(),
        }
    }
}

pub(super) fn register(registry: &mut ImportRegistry, state: &Arc<ServicesState>) {
    let submit = Arc::clone(state);
    registry.register(MODULE, "SubmitFixtureTask", move |args, cpu, _| {
        let input = args.rcx;
        let now = Duration::from_millis(submit.uptime_millis());
        let deadline = (args.rdx != 0).then(|| now + Duration::from_millis(args.rdx));
        let mut tasks = submit.task_fixture.lock().expect("fixture task queue");
        let delivered = Arc::clone(&tasks.delivered);
        match tasks.queue.submit_cancellable(
            deadline,
            move |token| {
                if token.is_cancelled() {
                    return Err(995);
                }
                input.checked_mul(input).ok_or(ERROR_INVALID_PARAMETER)
            },
            move |id, result| {
                *delivered.lock().expect("fixture task completion") = Some((id, result));
            },
        ) {
            Ok(id) => Ok(id.raw()),
            Err(_) => submit.fail(cpu.gs_base, ERROR_NOT_ENOUGH_MEMORY, 0),
        }
    });

    let cancel = Arc::clone(state);
    registry.register(MODULE, "CancelFixtureTask", move |args, _, _| {
        let mut tasks = cancel.task_fixture.lock().expect("fixture task queue");
        Ok(u64::from(tasks.queue.cancel(TaskId::from_raw(args.rcx))))
    });

    let work = Arc::clone(state);
    registry.register(MODULE, "DispatchFixtureWork", move |_, _, _| {
        let now = Duration::from_millis(work.uptime_millis());
        let mut tasks = work.task_fixture.lock().expect("fixture task queue");
        Ok(u64::from(tasks.queue.dispatch_work(now)))
    });

    let background = Arc::clone(state);
    registry.register(MODULE, "DispatchFixtureBackground", move |_, cpu, _| {
        let now = Duration::from_millis(background.uptime_millis());
        let mut tasks = background.task_fixture.lock().expect("fixture task queue");
        match tasks.queue.dispatch_background(now) {
            Ok(started) => Ok(u64::from(started)),
            Err(_) => background.fail(cpu.gs_base, ERROR_NOT_ENOUGH_MEMORY, 0),
        }
    });

    let completion = Arc::clone(state);
    registry.register(
        MODULE,
        "DispatchFixtureCompletion",
        move |args, cpu, memory| {
            if args.rcx == 0 || memory.check_write(args.rcx, COMPLETION_SIZE).is_err() {
                return completion.fail(cpu.gs_base, ERROR_NOACCESS, 0);
            }
            let mut tasks = completion.task_fixture.lock().expect("fixture task queue");
            tasks
                .queue
                .advance_time(Duration::from_millis(completion.uptime_millis()));
            if !tasks.queue.dispatch_completion() {
                return Ok(0);
            }
            let (id, result) = tasks
                .delivered
                .lock()
                .expect("fixture task completion")
                .take()
                .expect("dispatch delivers a completion");
            let (status, value): (u32, u64) = match result {
                TaskResult::Finished(Ok(value)) => (0, value),
                TaskResult::Finished(Err(error)) => (error, 0),
                TaskResult::Cancelled => (995, 0),
                TaskResult::TimedOut => (1460, 0),
                TaskResult::WorkerPanicked => (31, 0),
            };
            let mut record = [0; COMPLETION_SIZE];
            record[..8].copy_from_slice(&id.raw().to_le_bytes());
            record[8..12].copy_from_slice(&status.to_le_bytes());
            record[16..24].copy_from_slice(&value.to_le_bytes());
            memory
                .write_bytes(args.rcx, &record)
                .map_err(crate::cpu::ExitReason::MemoryFault)?;
            Ok(1)
        },
    );
}
