//! The executor receives IDs of operations that are ready for execution, executes
//! them, and places the result back into the fabric for further executions

use std::collections::HashMap;
use std::sync::Arc;

use crossbeam::queue::SegQueue;
use itertools::Itertools;
use tracing::log;

use crate::buffer::GrowableBuffer;
use crate::fabric::result::ERR_RESULT_BUFFER_POISONED;
use crate::network::NetworkOutbound;

use super::result::ResultWaiter;
use super::{result::OpResult, FabricInner};
use super::{Operation, OperationType, ResultId};

/// The executor is responsible for executing operation that are ready for execution, either
/// passed explicitly by the fabric or as a result of a dependency being satisfied
pub struct Executor {
    /// The job queue for the executor
    ///
    /// TODO: Use an `ArrayQueue` here for slightly improved performance
    job_queue: Arc<SegQueue<ExecutorMessage>>,
    /// The operation buffer, stores in-flight operations
    operations: GrowableBuffer<Operation>,
    /// The dependency map; maps in-flight results to operations that are waiting for them
    dependencies: GrowableBuffer<Vec<ResultId>>,
    /// The completed results of operations
    results: GrowableBuffer<OpResult>,
    /// An index of waiters for incomplete results
    waiters: HashMap<ResultId, Vec<ResultWaiter>>,
    /// The underlying fabric that the executor is a part of
    fabric: FabricInner,
    /// The total sampled queue length of the executor's work queue
    #[cfg(feature = "debug_info")]
    summed_queue_length: u64,
    /// The number of samples taken of the executor's work queue length
    #[cfg(feature = "debug_info")]
    queue_length_sample_count: usize,
}

/// The type that the `Executor` receives on its channel, this may either be:
/// - A result of an operation, for which th executor will check the dependency map and
///  execute any operations that are now ready
/// - An operation directly, which the executor will execute immediately if all of its
///  arguments are ready
#[derive(Debug)]
pub enum ExecutorMessage {
    /// A result of an operation
    Result(OpResult),
    /// An operation that is ready for execution
    Op(Operation),
    /// A new waiter has registered itself for a result
    NewWaiter(ResultWaiter),
    /// Indicates that the executor should shut down
    Shutdown,
}

impl Executor {
    /// Constructor
    pub fn new(
        circuit_size_hint: usize,
        job_queue: Arc<SegQueue<ExecutorMessage>>,
        fabric: FabricInner,
    ) -> Self {
        #[cfg(feature = "debug_info")]
        {
            Self {
                job_queue,
                operations: GrowableBuffer::new(circuit_size_hint),
                dependencies: GrowableBuffer::new(circuit_size_hint),
                results: GrowableBuffer::new(circuit_size_hint),
                waiters: HashMap::new(),
                fabric,
                summed_queue_length: 0,
                queue_length_sample_count: 0,
            }
        }

        #[cfg(not(feature = "debug_info"))]
        {
            Self {
                job_queue,
                operations: GrowableBuffer::new(circuit_size_hint),
                dependencies: GrowableBuffer::new(circuit_size_hint),
                results: GrowableBuffer::new(circuit_size_hint),
                waiters: HashMap::new(),
                fabric,
            }
        }
    }

    /// Run the executor until a shutdown message is received
    pub fn run(mut self) {
        loop {
            if let Some(job) = self.job_queue.pop() {
                match job {
                    ExecutorMessage::Result(res) => self.handle_new_result(res),
                    ExecutorMessage::Op(operation) => self.handle_new_operation(operation),
                    ExecutorMessage::NewWaiter(waiter) => self.handle_new_waiter(waiter),
                    ExecutorMessage::Shutdown => {
                        log::debug!("executor shutting down");

                        // In benchmarks print the average queue length
                        #[cfg(feature = "debug_info")]
                        {
                            println!("average queue length: {}", self.avg_queue_length());
                        }

                        break;
                    }
                }
            }

            #[cfg(feature = "debug_info")]
            {
                self.summed_queue_length += self.job_queue.len() as u64;
                self.queue_length_sample_count += 1;
            }
        }
    }

    /// Returns the average queue length over the execution of the executor
    #[cfg(feature = "debug_info")]
    pub fn avg_queue_length(&self) -> f64 {
        (self.summed_queue_length as f64) / (self.queue_length_sample_count as f64)
    }

    /// Handle a new result
    fn handle_new_result(&mut self, result: OpResult) {
        let id = result.id;

        // Lock the fabric elements needed
        let prev = self.results.insert(result.id, result);
        assert!(prev.is_none(), "duplicate result id: {id:?}");

        // Execute any ready dependencies
        if let Some(deps) = self.dependencies.get(id) {
            let mut ready_ops = Vec::new();
            for op_id in deps.iter() {
                {
                    let mut operation = self.operations.get_mut(*op_id).unwrap();

                    operation.inflight_args -= 1;
                    if operation.inflight_args > 0 {
                        continue;
                    }
                } // explicitly drop the mutable `self` reference

                // Mark the operation as ready for execution
                ready_ops.push(*op_id);
            }

            for op in ready_ops.into_iter() {
                let op = self.operations.take(op).unwrap();
                self.execute_operation(op);
            }
        }

        self.wake_waiters_on_result(id);
    }

    /// Handle a new operation
    fn handle_new_operation(&mut self, mut op: Operation) {
        // Check if all arguments are ready
        let n_ready = op
            .args
            .iter()
            .filter_map(|id| self.results.get(*id))
            .count();
        let inflight_args = op.args.len() - n_ready;
        op.inflight_args = inflight_args;

        // If the operation is ready for execution, do so
        if inflight_args == 0 {
            self.execute_operation(op);
            return;
        }

        // Otherwise, add the operation to the in-flight operations list and the dependency map
        for arg in op.args.iter() {
            let entry = self.dependencies.entry_mut(*arg);
            if entry.is_none() {
                *entry = Some(Vec::new());
            }

            entry.as_mut().unwrap().push(op.id);
        }

        self.operations.insert(op.id, op);
    }

    /// Executes an operation whose arguments are ready
    fn execute_operation(&mut self, op: Operation) {
        let result_ids = op.result_ids();

        // Collect the inputs to the operation
        let inputs = op
            .args
            .iter()
            .map(|arg| self.results.get(*arg).unwrap().value.clone())
            .collect_vec();

        match op.op_type {
            OperationType::Gate { function } => {
                let value = (function)(inputs);
                self.handle_new_result(OpResult {
                    id: op.result_id,
                    value,
                });
            }

            OperationType::GateBatch { function } => {
                let output = (function)(inputs);
                result_ids
                    .into_iter()
                    .zip(output.into_iter())
                    .map(|(id, value)| OpResult { id, value })
                    .for_each(|res| self.handle_new_result(res));
            }

            OperationType::Network { function } => {
                // Derive a network payload from the gate inputs and forward it to the outbound buffer
                let result_id = result_ids[0];
                let payload = (function)(inputs);
                let outbound = NetworkOutbound {
                    result_id,
                    payload: payload.clone(),
                };

                self.fabric
                    .outbound_queue
                    .send(outbound)
                    .expect("error sending network payload");

                // On a `send`, the local party receives a copy of the value placed as the result of
                // the network operation, so we must re-enqueue the result
                self.handle_new_result(OpResult {
                    id: result_id,
                    value: payload.into(),
                });
            }
        };
    }

    /// Handle a new waiter for a result
    pub fn handle_new_waiter(&mut self, waiter: ResultWaiter) {
        let id = waiter.result_id;

        // Insert the new waiter to the queue
        self.waiters
            .entry(waiter.result_id)
            .or_insert_with(Vec::new)
            .push(waiter);

        // If the result being awaited is already available, wake the waiter
        if self.results.get(id).is_some() {
            self.wake_waiters_on_result(id);
        }
    }

    /// Wake all the waiters for a given result
    pub fn wake_waiters_on_result(&mut self, result_id: ResultId) {
        // Wake all tasks awaiting this result
        if let Some(waiters) = self.waiters.remove(&result_id) {
            let result = &self.results.get(result_id).unwrap().value;
            for waiter in waiters.into_iter() {
                // Place the result in the waiter's buffer and wake up the waiting thread
                let mut buffer = waiter
                    .result_buffer
                    .write()
                    .expect(ERR_RESULT_BUFFER_POISONED);

                buffer.replace(result.clone());
                waiter.waker.wake();
            }
        }
    }
}
