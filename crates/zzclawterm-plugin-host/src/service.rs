use crate::manager::{CatalogSnapshot, Invocation, PluginManager, PreparedChange};
use futures::channel::{mpsc as async_channel, oneshot};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{self, Sender},
};
use std::thread::{self, JoinHandle};
use zzclawterm_core::plugins::invocation::ActionInput;
use zzclawterm_core::plugins::{ErrorCode, PluginError, PluginResult};
use zzclawterm_core::runtime::AppRuntime;

pub enum PluginOperation {
    Install {
        source: PathBuf,
        development: bool,
    },
    Update {
        id: String,
        source: PathBuf,
    },
    Enable {
        id: String,
        enabled: bool,
    },
    Reload {
        id: String,
    },
    Uninstall {
        id: String,
    },
    Invoke {
        contribution_id: String,
        expected_revision: u64,
        input: ActionInput,
    },
}
pub enum OperationReply {
    Changed,
    Invocation(Invocation),
}
pub enum PluginEvent {
    CatalogChanged,
}
pub type OperationTask = oneshot::Receiver<PluginResult<OperationReply>>;
struct Request {
    operation: PluginOperation,
    reply: oneshot::Sender<PluginResult<OperationReply>>,
}

#[cfg(test)]
#[derive(Default)]
struct TestHooks {
    before_dispatch: Option<DispatchHook>,
    after_start: Option<DispatchHook>,
    before_prepare: Option<DispatchHook>,
}
#[cfg(test)]
type DispatchHook = Arc<dyn Fn(&PluginOperation) + Send + Sync>;

fn shutdown_error() -> PluginError {
    PluginError::new(ErrorCode::Shutdown, "Plugin service is shutting down")
}

enum Message {
    Request(Request),
    Prepared(u64, Box<PluginResult<PreparedChange>>),
    RuntimeChanged,
    Shutdown,
}

struct Preparing {
    // None reserves installation order until the new package ID is known.
    id: Option<String>,
    reply: oneshot::Sender<PluginResult<OperationReply>>,
    worker: JoinHandle<()>,
}

const QUEUE_CAPACITY: usize = 16;
const PREPARATION_WORKERS: usize = 2;

/// One process-level registry actor. Package snapshots, compilation and guest
/// initialization run on at most two preparation threads; commits stay serial.
/// Requests for the same plugin retain submission order, while other plugins
/// can still invoke or prepare. No worker wakes periodically while idle.
pub struct PluginService {
    requests: Sender<Message>,
    queued: Arc<AtomicUsize>,
    snapshot: Arc<RwLock<CatalogSnapshot>>,
    stop: Arc<AtomicBool>,
    dispatch: Arc<Mutex<()>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl PluginService {
    pub fn start(
        runtime: AppRuntime,
    ) -> PluginResult<(Arc<Self>, async_channel::Receiver<PluginEvent>)> {
        Self::start_inner(
            runtime,
            #[cfg(test)]
            crate::runtime::RuntimeLimits::default(),
            #[cfg(test)]
            TestHooks::default(),
        )
    }

    fn start_inner(
        runtime: AppRuntime,
        #[cfg(test)] limits: crate::runtime::RuntimeLimits,
        #[cfg(test)] hooks: TestHooks,
    ) -> PluginResult<(Arc<Self>, async_channel::Receiver<PluginEvent>)> {
        // The admission counter bounds user requests independently of internal
        // completions, so shutdown/fault notifications cannot be lost to a full queue.
        let (requests, receiver) = mpsc::channel::<Message>();
        let queued = Arc::new(AtomicUsize::new(0));
        let (mut events, event_receiver) = async_channel::channel(16);
        let snapshot = Arc::new(RwLock::new(CatalogSnapshot::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let dispatch = Arc::new(Mutex::new(()));
        let snapshot_for_worker = snapshot.clone();
        let stop_for_worker = stop.clone();
        let dispatch_for_worker = dispatch.clone();
        let queued_for_worker = queued.clone();
        let completion = requests.clone();
        let worker = thread::Builder::new()
            .name("plugin-manager".into())
            .spawn(move || {
                #[cfg(not(test))]
                let opened = PluginManager::for_runtime(&runtime);
                #[cfg(test)]
                let opened = PluginManager::open(
                    runtime.data_dir().join("plugins"),
                    env!("CARGO_PKG_VERSION"),
                    limits,
                );
                let mut manager = match opened {
                    Ok(manager) => {
                        let changed = completion.clone();
                        let notified = Arc::new(AtomicBool::new(false));
                        let flag = notified.clone();
                        manager.set_change_handler(Arc::new(move || {
                            if !flag.swap(true, Ordering::AcqRel) {
                                let _ = changed.send(Message::RuntimeChanged);
                            }
                        }));
                        Some((manager, notified))
                    }
                    Err(error) => {
                        snapshot_for_worker.write().unwrap().startup_error = Some(error);
                        let _ = events.try_send(PluginEvent::CatalogChanged);
                        None
                    }
                };
                let mut pending = VecDeque::<Request>::new();
                let mut preparing = BTreeMap::<u64, Preparing>::new();
                let mut sequence = 0;
                if let Some((manager, _)) = &manager {
                    publish(manager, &snapshot_for_worker, &mut events);
                }
                loop {
                    // Scan past a busy plugin without reordering requests that
                    // target that plugin. Installation is a management barrier.
                    let mut index = 0;
                    while index < pending.len() {
                        if stop_for_worker.load(Ordering::Acquire) {
                            let request = pending.remove(index).unwrap();
                            queued_for_worker.fetch_sub(1, Ordering::AcqRel);
                            let _ = request.reply.send(Err(shutdown_error()));
                            continue;
                        }
                        let operation = &pending[index].operation;
                        let id = operation_id(operation);
                        let invoke = matches!(operation, PluginOperation::Invoke { .. });
                        let blocked = preparing.values().any(|job| {
                            if invoke {
                                job.id.as_deref().is_some_and(|busy| Some(busy) == id)
                            } else {
                                job.id.is_none() || id.is_none() || job.id.as_deref() == id
                            }
                        }) || pending.iter().take(index).any(|earlier| {
                            if invoke {
                                operation_id(&earlier.operation) == id
                            } else {
                                let earlier_id = operation_id(&earlier.operation);
                                earlier_id.is_none() || id.is_none() || earlier_id == id
                            }
                        });
                        if blocked || (!invoke && preparing.len() >= PREPARATION_WORKERS) {
                            index += 1;
                            continue;
                        }
                        let request = pending.remove(index).unwrap();
                        queued_for_worker.fetch_sub(1, Ordering::AcqRel);
                        #[cfg(test)]
                        if let Some(hook) = &hooks.before_dispatch {
                            hook(&request.operation);
                        }
                        {
                            let _dispatch = dispatch_for_worker.lock().unwrap();
                            if stop_for_worker.load(Ordering::Acquire) {
                                let _ = request.reply.send(Err(shutdown_error()));
                                continue;
                            }
                            if invoke && request.reply.is_canceled() {
                                continue;
                            }
                        }
                        #[cfg(test)]
                        if let Some(hook) = &hooks.after_start {
                            hook(&request.operation);
                        }
                        let Some((manager, _)) = &mut manager else {
                            let error = snapshot_for_worker
                                .read()
                                .unwrap()
                                .startup_error
                                .clone()
                                .unwrap();
                            let _ = request.reply.send(Err(error));
                            continue;
                        };
                        let plan = match &request.operation {
                            PluginOperation::Install {
                                source,
                                development,
                            } => Some(manager.plan_replace(source, *development, None)),
                            PluginOperation::Update { id, source } => {
                                Some(manager.plan_update(id, source))
                            }
                            PluginOperation::Enable { id, enabled: true } => {
                                Some(manager.plan_load(id, true))
                            }
                            PluginOperation::Reload { id } => Some(manager.plan_load(id, false)),
                            _ => None,
                        };
                        if let Some(plan) = plan {
                            let plan = match plan {
                                Ok(plan) => plan,
                                Err(error) => {
                                    let _ = request.reply.send(Err(error));
                                    continue;
                                }
                            };
                            sequence += 1;
                            let key = sequence;
                            let done = completion.clone();
                            let job_id = operation_id(&request.operation).map(str::to_owned);
                            #[cfg(test)]
                            let prepare_hook = hooks.before_prepare.clone();
                            let worker = thread::Builder::new()
                                .name("plugin-prepare".into())
                                .spawn(move || {
                                    let result = std::panic::catch_unwind(
                                        std::panic::AssertUnwindSafe(|| {
                                            #[cfg(test)]
                                            if let Some(hook) = prepare_hook {
                                                hook(&request.operation);
                                            }
                                            plan.run()
                                        }),
                                    )
                                    .unwrap_or_else(|_| {
                                        Err(PluginError::new(
                                            ErrorCode::Initialization,
                                            "Plugin preparation worker failed",
                                        ))
                                    });
                                    let _ = done.send(Message::Prepared(key, Box::new(result)));
                                });
                            match worker {
                                Ok(worker) => {
                                    preparing.insert(
                                        key,
                                        Preparing {
                                            id: job_id,
                                            reply: request.reply,
                                            worker,
                                        },
                                    );
                                }
                                Err(_) => {
                                    let _ = request.reply.send(Err(PluginError::new(
                                        ErrorCode::Initialization,
                                        "Cannot start plugin preparation worker",
                                    )));
                                }
                            }
                        } else {
                            let result = match request.operation {
                                PluginOperation::Enable { id, enabled: false } => manager
                                    .set_enabled(&id, false)
                                    .map(|_| OperationReply::Changed),
                                PluginOperation::Uninstall { id } => {
                                    manager.uninstall(&id).map(|_| OperationReply::Changed)
                                }
                                PluginOperation::Invoke {
                                    contribution_id,
                                    expected_revision,
                                    input,
                                } => manager
                                    .invoke_at_revision(&contribution_id, expected_revision, input)
                                    .map(OperationReply::Invocation),
                                _ => unreachable!(),
                            };
                            publish(manager, &snapshot_for_worker, &mut events);
                            let _ = request.reply.send(result);
                        }
                    }
                    if stop_for_worker.load(Ordering::Acquire) && preparing.is_empty() {
                        break;
                    }
                    match receiver.recv() {
                        Ok(Message::Request(request)) => pending.push_back(request),
                        Ok(Message::Prepared(key, result)) => {
                            let job = preparing.remove(&key).unwrap();
                            let _ = job.worker.join();
                            let (manager, _) = manager.as_mut().unwrap();
                            // An accepted, started transaction finishes or rolls
                            // back even when shutdown has closed submission.
                            let result = (*result)
                                .and_then(|prepared| manager.commit(prepared))
                                .map(|_| OperationReply::Changed);
                            publish(manager, &snapshot_for_worker, &mut events);
                            let _ = job.reply.send(result);
                        }
                        Ok(Message::RuntimeChanged) => {
                            if let Some((manager, notified)) = &manager {
                                notified.store(false, Ordering::Release);
                                publish(manager, &snapshot_for_worker, &mut events);
                            }
                        }
                        Ok(Message::Shutdown) => {}
                        Err(_) => break,
                    }
                }
                {
                    let _dispatch = dispatch_for_worker.lock().unwrap();
                    stop_for_worker.store(true, Ordering::Release);
                }
                while let Ok(message) = receiver.try_recv() {
                    if let Message::Request(request) = message {
                        queued_for_worker.fetch_sub(1, Ordering::AcqRel);
                        let _ = request.reply.send(Err(shutdown_error()));
                    }
                }
                let final_snapshot = if let Some((mut manager, _)) = manager.take() {
                    manager.shutdown();
                    let snapshot = manager.snapshot();
                    drop(manager);
                    snapshot
                } else {
                    CatalogSnapshot {
                        stopped: true,
                        ..snapshot_for_worker.read().unwrap().clone()
                    }
                };
                *snapshot_for_worker.write().unwrap() = final_snapshot;
                let _ = events.try_send(PluginEvent::CatalogChanged);
            })
            .map_err(|_| {
                PluginError::new(
                    ErrorCode::Initialization,
                    "Cannot start plugin management worker",
                )
            })?;
        Ok((
            Arc::new(Self {
                requests,
                queued,
                snapshot,
                stop,
                dispatch,
                worker: Mutex::new(Some(worker)),
            }),
            event_receiver,
        ))
    }

    pub fn snapshot(&self) -> CatalogSnapshot {
        self.snapshot.read().unwrap().clone()
    }

    pub fn submit(&self, operation: PluginOperation) -> PluginResult<OperationTask> {
        let _dispatch = self.dispatch.lock().unwrap();
        if self.stop.load(Ordering::Acquire) {
            return Err(shutdown_error());
        }
        if self.queued.load(Ordering::Acquire) >= QUEUE_CAPACITY {
            return Err(PluginError::new(
                ErrorCode::QueueFull,
                "Plugin management queue is full; wait and retry",
            ));
        }
        self.queued.fetch_add(1, Ordering::AcqRel);
        let (reply, task) = oneshot::channel();
        if self
            .requests
            .send(Message::Request(Request { operation, reply }))
            .is_err()
        {
            self.queued.fetch_sub(1, Ordering::AcqRel);
            return Err(shutdown_error());
        }
        Ok(task)
    }

    /// Call off the UI thread, before AppShell quits the process.
    pub fn shutdown(&self) {
        self.begin_shutdown();
        let mut worker = self.worker.lock().unwrap();
        if let Some(worker) = worker.take() {
            let _ = worker.join();
        }
    }

    fn begin_shutdown(&self) {
        let _dispatch = self.dispatch.lock().unwrap();
        if !self.stop.swap(true, Ordering::AcqRel) {
            let _ = self.requests.send(Message::Shutdown);
        }
    }
}

fn operation_id(operation: &PluginOperation) -> Option<&str> {
    match operation {
        PluginOperation::Install { .. } => None,
        PluginOperation::Update { id, .. }
        | PluginOperation::Enable { id, .. }
        | PluginOperation::Reload { id }
        | PluginOperation::Uninstall { id } => Some(id),
        PluginOperation::Invoke {
            contribution_id, ..
        } => contribution_id.split_once(':').map(|(id, _)| id),
    }
}

fn publish(
    manager: &PluginManager,
    snapshot: &RwLock<CatalogSnapshot>,
    events: &mut async_channel::Sender<PluginEvent>,
) {
    let current = manager.snapshot();
    if *snapshot.read().unwrap() != current {
        *snapshot.write().unwrap() = current;
        // Coalescing is safe: readers always fetch the latest snapshot.
        let _ = events.try_send(PluginEvent::CatalogChanged);
    }
}

impl Drop for PluginService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests;
