//! Several programs in several tasks, on one thread (ADR-013 §2.19).
//!
//! A runtime has tasks, and a task runs programs in the order written. One thread, the
//! scheduler, runs every task: a program's run is a [`Run`] continued in pieces of [`PIECE`]
//! instructions, and between two pieces [`sched::next`] says which task to continue or begin,
//! so a task of higher priority preempts one of lower without any thread priority of the
//! operating system. A runtime given no tasks runs its one program in its implied task, on the
//! same path, and reports exactly what it reported before there were tasks.
//!
//! Each program has two copies of its memory:
//! - its **working memory**, which only the scheduler thread holds and a run works in, with no
//!   lock held;
//! - its **image**, in [`Shared`], the memory as the program's last completed run left it. An
//!   exchange, a watch, the RETAIN values and the values of an online change read images, so
//!   none ever sees a program half-way through a run.
//!
//! When a task begins a run, the inputs of all its programs are taken from the last exchange,
//! its setpoints and its forces written (the latch); when its run ends, its forces are written
//! again and each of its programs' working memory is copied to its image.
//!
//! A data block (ADR-014) has one **master** copy per runtime, in [`Shared`], and each program
//! that declares it has its own copy in its memory. When a program begins its run, each member
//! it does not write is copied into its copy: from the working memory of the member's writer
//! when that writer is a program of the same task that has already ended its run in this task
//! run, otherwise from the master. Nothing reaches the master before a task's run has ended:
//! then, with the images and under the same lock, each program of the task copies the members
//! it writes into the master. So a task of another priority sees a task's run whole or not at
//! all, and a task run that faults, or that a change of mode drops, hands nothing over. Every
//! copy is made by the scheduler thread between two pieces, never while a piece runs.

use crate::{describe, restore, snapshot, to_vm, watchdog_limit, Plan, BUDGET, DEFAULT_WATCHDOG};
use mythos_logic_vm::sched::{self, Kind, Next};
use mythos_logic_vm::{cold_start, write, ElemType, Frame, Piece, Program, Run, Value as VmValue};
use mythos_module::protocol::{
    CarryReport, DataBlock, ForcedValue, InputOf, ProgramVersion, RetainedValue, RuntimeMode,
    RuntimeState, TaskConfig, TaskKind, TaskState,
};
use mythos_plugin_sdk::PluginError;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Instructions in one piece of a run: between two pieces the scheduler may begin a task of
/// higher priority. About 0.1 ms at the ~10 ns an instruction measured on one PC
/// (ADR-013-evidence/04); how long a task of the highest priority actually waits for a piece
/// to end is NOT MEASURED.
pub(crate) const PIECE: u32 = 10_000;

/// The longest the scheduler sleeps before it looks again, so that it ends promptly when asked
/// to, whatever a task's interval.
const IDLE_WAIT: Duration = Duration::from_millis(20);

/// One task as loaded: its configuration, its programs (indices into [`Layout::plans`]) and its
/// watchdog's limit.
pub(crate) struct TaskSpec {
    pub config: TaskConfig,
    pub programs: Vec<usize>,
    pub watchdog: Duration,
    /// The tag whose rising edge triggers it, for an event task.
    pub event_tag: Option<String>,
}

/// One data block of a runtime as loaded: its configuration, who declares it and who writes
/// each of its members. A program's use of it is `(program, index into its data)`.
pub(crate) struct BlockSpec {
    pub config: DataBlock,
    /// The first program, in configuration order, that declares it: its master's first image
    /// is that program's copy. `None` for a block no program declares, which has no master.
    pub first: Option<(usize, usize)>,
    /// Every program that declares it.
    pub uses: Vec<(usize, usize)>,
    /// For each member, in the order of `config.members`, the one program that writes it.
    pub writer: Vec<Option<(usize, usize)>>,
}

/// What a runtime runs: its programs and the tasks that run them. Fixed from the start.
pub(crate) struct Layout {
    pub plans: Vec<Plan>,
    pub tasks: Vec<TaskSpec>,
    /// Given no tasks: one program in its implied task, reported exactly as before tasks.
    pub implied: bool,
    /// The data blocks; empty for a runtime of today.
    pub blocks: Vec<BlockSpec>,
    /// For each program, for each block it declares (in the order of its data), the index of
    /// that block in `blocks`.
    block_of: Vec<Vec<usize>>,
    /// For each program, its task and its place in that task's programs.
    place: Vec<(usize, usize)>,
    /// The scheduler's tasks, in the order of `tasks`, at a change to RUN.
    sched: Vec<sched::Task>,
    /// The same at the start of an online change: its startup tasks are event tasks never
    /// triggered, so they do not run.
    sched_online: Vec<sched::Task>,
}

/// Milliseconds as the scheduler's nanoseconds; `check_tasks` refused any that do not fit.
fn ns_of_ms(ms: u64) -> i64 {
    i64::try_from(ms.saturating_mul(1_000_000)).unwrap_or(i64::MAX)
}

fn us_of_ns(ns: i64) -> u64 {
    (ns.max(0) / 1_000) as u64
}

impl Layout {
    /// `configs` have passed [`mythos_module::protocol::check_tasks`] against `plans`, and
    /// `data` [`mythos_module::protocol::check_data`].
    pub fn new(
        plans: Vec<Plan>,
        configs: Vec<TaskConfig>,
        implied: bool,
        data: Vec<DataBlock>,
    ) -> Result<Layout, String> {
        let mut tasks = Vec::new();
        let mut normal = Vec::new();
        let mut online = Vec::new();
        for config in configs {
            let name = config.name.clone();
            let programs = config
                .programs
                .iter()
                .map(|p| {
                    plans
                        .iter()
                        .position(|plan| plan.program.name.eq_ignore_ascii_case(p))
                        .ok_or_else(|| format!("task '{name}' runs '{p}', which is not loaded"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let (kind, watchdog, event_tag) = match &config.kind {
                TaskKind::Cycle { min_cycle_ms } => (
                    Kind::Cycle {
                        min_cycle: ns_of_ms(*min_cycle_ms),
                    },
                    watchdog_limit(*min_cycle_ms),
                    None,
                ),
                TaskKind::Cyclic {
                    interval_ms,
                    phase_ms,
                } => (
                    Kind::Cyclic {
                        interval: ns_of_ms(*interval_ms),
                        phase: ns_of_ms(*phase_ms),
                    },
                    watchdog_limit(*interval_ms),
                    None,
                ),
                TaskKind::Startup {} => (Kind::Startup, DEFAULT_WATCHDOG, None),
                TaskKind::Event { tag } => {
                    // An edge is seen in a BOOL the program reads: the module knows the types.
                    let bound = plans.iter().any(|plan| {
                        plan.inputs.iter().any(|(var, t, of)| {
                            t == tag && *of == InputOf::Value && var.ty == ElemType::Bool
                        })
                    });
                    if !bound {
                        return Err(format!(
                            "task '{name}': its tag {tag} is not read into a BOOL variable by any program of this runtime"
                        ));
                    }
                    (Kind::Event, DEFAULT_WATCHDOG, Some(tag.clone()))
                }
            };
            let watchdog = config
                .watchdog_ms
                .map(Duration::from_millis)
                .unwrap_or(watchdog);
            let task = |kind| {
                sched::Task::new(kind, config.priority)
                    .ok_or_else(|| format!("task '{name}' cannot be scheduled"))
            };
            normal.push(task(kind)?);
            online.push(task(if kind == Kind::Startup {
                Kind::Event
            } else {
                kind
            })?);
            tasks.push(TaskSpec {
                config,
                programs,
                watchdog,
                event_tag,
            });
        }
        let mut place = vec![None; plans.len()];
        for (k, t) in tasks.iter().enumerate() {
            for (at, &p) in t.programs.iter().enumerate() {
                place[p] = Some((k, at));
            }
        }
        let place = place
            .into_iter()
            .enumerate()
            .map(|(p, at)| {
                at.ok_or_else(|| format!("program '{}' is in no task", plans[p].program.name))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut blocks: Vec<BlockSpec> = data
            .into_iter()
            .map(|config| BlockSpec {
                writer: vec![None; config.members.len()],
                config,
                first: None,
                uses: Vec::new(),
            })
            .collect();
        let mut block_of = Vec::new();
        for (p, plan) in plans.iter().enumerate() {
            let mut of = Vec::new();
            for (j, u) in plan.program.data.iter().enumerate() {
                let b = blocks
                    .iter()
                    .position(|b| b.config.name.eq_ignore_ascii_case(&u.name))
                    .ok_or_else(|| {
                        format!("data block '{}' is not in the runtime's data", u.name)
                    })?;
                let spec = &mut blocks[b];
                if u.members.len() != spec.writer.len() {
                    return Err(format!("data block '{}' has other members", u.name));
                }
                spec.first.get_or_insert((p, j));
                spec.uses.push((p, j));
                for (i, m) in u.members.iter().enumerate() {
                    if m.written {
                        spec.writer[i] = Some((p, j));
                    }
                }
                of.push(b);
            }
            block_of.push(of);
        }
        Ok(Layout {
            plans,
            tasks,
            implied,
            blocks,
            block_of,
            place,
            sched: normal,
            sched_online: online,
        })
    }

    /// Whether program `p` declares a data block.
    fn declares(&self, p: usize) -> bool {
        !self.plans[p].program.data.is_empty()
    }

    /// Where program `p`'s copy of member `i` of the block of its `j`-th use lies in its
    /// memory, as a range. Inside it: `plan` refused any copy outside (R14) or any member
    /// outside its block (R5), and every use's members are its block's (R9).
    fn member_range(&self, p: usize, j: usize, i: usize) -> std::ops::Range<usize> {
        let u = &self.plans[p].program.data[j];
        let m = &u.members[i];
        let at = u.offset as usize + m.offset as usize;
        at..at + m.size as usize
    }

    /// Where member `i` of block `b` lies in its master.
    fn master_range(&self, b: usize, i: usize) -> std::ops::Range<usize> {
        let m = &self.blocks[b].config.members[i];
        m.offset as usize..m.offset as usize + m.size as usize
    }

    /// Where program `p`'s copy of the block of its `j`-th use lies in its memory.
    fn copy_range(&self, p: usize, j: usize) -> std::ops::Range<usize> {
        let u = &self.plans[p].program.data[j];
        u.offset as usize..u.offset as usize + u.size as usize
    }

    /// Whether a variable is named by its program: the runtime has several.
    pub fn several(&self) -> bool {
        self.plans.len() > 1
    }

    /// A path of program `p` as the runtime names it.
    pub fn full_name(&self, p: usize, path: &str) -> String {
        if self.several() {
            format!("{}.{path}", self.plans[p].program.name)
        } else {
            path.to_string()
        }
    }

    fn program_names(&self) -> String {
        self.plans
            .iter()
            .map(|p| p.program.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Which program a name is of, and its path in that program. With one program the name is
    /// the path; with several it is `<program>.<path>`, the program without regard to case.
    pub fn split<'a>(&self, name: &'a str) -> Result<(usize, &'a str), String> {
        if !self.several() {
            return Ok((0, name));
        }
        let Some((program, path)) = name.split_once('.') else {
            return Err(format!(
                "name the program first, as in {}.{} (this runtime runs {})",
                self.plans[0].program.name,
                name.trim(),
                self.program_names()
            ));
        };
        let program = program.trim();
        self.plans
            .iter()
            .position(|p| p.program.name.eq_ignore_ascii_case(program))
            .map(|p| (p, path))
            .ok_or_else(|| {
                format!(
                    "the runtime has no program {program}; it runs {}",
                    self.program_names()
                )
            })
    }
}

/// A variable held at a value: written before each run, so the program reads it, and after,
/// so what the program wrote does not stand.
pub(crate) struct Forced {
    /// The name as it was forced.
    pub variable: String,
    pub offset: u32,
    pub value: VmValue,
}

/// Write every forced variable its value.
pub(crate) fn hold(forces: &[Forced], memory: &mut [u8]) {
    for f in forces {
        write(memory, f.offset, f.value);
    }
}

/// One program's state that exchanges, watches and the scheduler share.
pub(crate) struct ProgramLive {
    /// Its memory as its last completed run left it, or as RUN or STOP set it.
    pub image: Vec<u8>,
    /// The variables held at a value. Only while in RUN: leaving it releases every one.
    pub forces: Vec<Forced>,
    /// Setpoints written since its last latch: taken into its working memory at the next.
    pub setpoints: Vec<(u32, VmValue)>,
    /// Whether it has been put in RUN since it was loaded: its image then holds its values.
    pub ran: bool,
    /// Until then, the RETAIN values the host gave for it, for its first warm start.
    pub pending: Vec<RetainedValue>,
    /// Whether its task has ended a run since the runtime was put in RUN.
    pub completed: bool,
}

impl ProgramLive {
    pub fn is_forced(&self, offset: u32) -> bool {
        self.forces.iter().any(|f| f.offset == offset)
    }
}

/// One task's state beside the scheduler's.
#[derive(Clone, Default)]
struct TaskLive {
    /// Since when it owes a run: what its watchdog measures. It can lie ahead, when the run it
    /// owes next is not yet due.
    owed_since: Option<Instant>,
    /// An exchange saw its trigger, and the scheduler has not yet taken it.
    triggered: bool,
    /// An event task's tag at the last exchange: `Some` only when it was GOOD and a BOOL.
    prev: Option<bool>,
    /// Triggers that made no run because one was already waiting.
    missed: u64,
}

/// One data block's state.
pub(crate) struct BlockLive {
    /// Its master copy: what each member's writer left at the end of its task's last completed
    /// run. `None` until the runtime is first put in RUN, and always for a block no program
    /// declares: nothing then says what its values are.
    pub master: Option<Vec<u8>>,
    /// Until then, the value the host kept for it, for its first warm start.
    pub pending: Option<RetainedValue>,
}

/// What the scheduler thread, the exchanges and every other call share, under one lock that is
/// never held across a piece of a run.
pub(crate) struct Shared {
    pub mode: RuntimeMode,
    pub fault: Option<String>,
    /// Changed by every change of mode and every fault: the scheduler drops what it was running.
    generation: u64,
    /// Whether an exchange has delivered the tags since the runtime was put in RUN.
    fed: bool,
    /// When the runtime was put in RUN: the zero of its clock.
    pub since: Instant,
    /// The tags of the last exchange, by URI.
    inputs: HashMap<String, (Option<Value>, bool)>,
    sched: Vec<sched::Task>,
    /// Whether the scheduler has started its tasks since the runtime was put in RUN.
    started: bool,
    /// When it started them, on the runtime's clock: the zero of a cyclic task's due times.
    started_at: i64,
    tasks: Vec<TaskLive>,
    pub programs: Vec<ProgramLive>,
    /// The data blocks, in the order of [`Layout::blocks`]. During RUN only the scheduler
    /// thread writes a master, always between pieces.
    pub blocks: Vec<BlockLive>,
    /// RETAIN values the host gave that name no program or data block of this runtime: given
    /// back until the runtime is first put in RUN, so none is lost to a runtime loaded in STOP.
    pub unclaimed: Vec<RetainedValue>,
    /// What it did with the running values it was started with, if any.
    pub carry: CarryReport,
}

/// [`Shared`] and the signal that wakes the scheduler.
pub(crate) struct Control {
    shared: Mutex<Shared>,
    wake: Condvar,
}

impl Control {
    pub fn new(shared: Shared) -> Control {
        Control {
            shared: Mutex::new(shared),
            wake: Condvar::new(),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Wake the scheduler: something it waits for may have changed.
    pub fn notify(&self) {
        self.wake.notify_all();
    }
}

/// Nanoseconds since `since`, the runtime's clock.
fn clock(since: Instant) -> i64 {
    since.elapsed().as_nanos().min(i64::MAX as u128) as i64
}

impl Shared {
    /// Loaded, in STOP, with the RETAIN values the host gave, each for the program it names.
    pub fn new(layout: &Layout, retained: Vec<RetainedValue>) -> Shared {
        let mut programs: Vec<ProgramLive> = layout
            .plans
            .iter()
            .map(|plan| ProgramLive {
                image: vec![0u8; plan.memory_size],
                forces: Vec::new(),
                setpoints: Vec::new(),
                ran: false,
                pending: Vec::new(),
                completed: false,
            })
            .collect();
        let mut blocks: Vec<BlockLive> = layout
            .blocks
            .iter()
            .map(|_| BlockLive {
                master: None,
                pending: None,
            })
            .collect();
        let mut unclaimed = Vec::new();
        for value in retained {
            if value.layout.is_some() {
                // A data block's master: to the block of that name, if there is one.
                let b = layout
                    .blocks
                    .iter()
                    .position(|b| b.config.name.eq_ignore_ascii_case(&value.path));
                match b {
                    Some(b) if blocks[b].pending.is_none() => blocks[b].pending = Some(value),
                    _ => unclaimed.push(value),
                }
                continue;
            }
            match layout.split(&value.path) {
                Ok((p, path)) => programs[p].pending.push(RetainedValue {
                    path: path.to_string(),
                    ..value
                }),
                Err(_) => unclaimed.push(value),
            }
        }
        Shared {
            mode: RuntimeMode::Stop,
            fault: None,
            generation: 0,
            fed: false,
            since: Instant::now(),
            inputs: HashMap::new(),
            sched: layout.sched.clone(),
            started: false,
            started_at: 0,
            tasks: vec![TaskLive::default(); layout.tasks.len()],
            programs,
            blocks,
            unclaimed,
            carry: CarryReport::default(),
        }
    }

    fn changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// The tasks as they are at a change to RUN; `online` at the start of an online change,
    /// when no startup task runs.
    pub fn begin_tasks(&mut self, layout: &Layout, online: bool) {
        self.sched = if online {
            layout.sched_online.clone()
        } else {
            layout.sched.clone()
        };
        self.started = false;
        self.tasks = vec![TaskLive::default(); layout.tasks.len()];
    }

    /// Put in RUN from STOP or FAULT: every program from its initial values, clock at zero,
    /// waiting for its first inputs. Warm unless `cold`: each RETAIN variable keeps the value
    /// it had when the program stopped, or the host gave if it has not run since it was
    /// loaded (IEC 61131-3 §6.5.6).
    pub fn restart(&mut self, layout: &Layout, cold: bool) -> Result<(), PluginError> {
        for (plan, live) in layout.plans.iter().zip(self.programs.iter_mut()) {
            let retained = if cold {
                None
            } else if live.ran {
                Some(snapshot(plan, &live.image))
            } else {
                Some(std::mem::take(&mut live.pending))
            };
            let program = Program::load(&plan.bytes).map_err(|e| {
                PluginError::RuntimeError(format!("the program does not load: {e:?}"))
            })?;
            cold_start(&program, &mut live.image)
                .map_err(|e| PluginError::RuntimeError(describe(e)))?;
            if let Some(values) = retained {
                for why in restore(plan, &mut live.image, &values) {
                    tracing::warn!("a retained value was not taken: {why}");
                }
            }
            live.ran = true;
            live.pending.clear();
            live.forces.clear();
            live.setpoints.clear();
            live.completed = false;
        }
        self.restart_blocks(layout, cold);
        for value in std::mem::take(&mut self.unclaimed) {
            if value.layout.is_some() {
                tracing::warn!(
                    "a retained value was not taken: {}: the runtime has no data block of that name",
                    value.path
                );
            } else {
                tracing::warn!(
                    "a retained value was not taken: {}: the runtime has no program of that name",
                    value.path
                );
            }
        }
        self.mode = RuntimeMode::Run;
        self.fault = None;
        self.fed = false;
        self.since = Instant::now();
        self.begin_tasks(layout, false);
        self.changed();
        Ok(())
    }

    /// Each data block's master at a change to RUN, once every program has its initial and
    /// RETAIN values: the copy of its first declaring program (the type's initial values); for
    /// a RETAIN block at a warm restart, the value kept, when it is of the block's layout and
    /// size. Then every program's copy is set from its master, so every copy agrees with it.
    fn restart_blocks(&mut self, layout: &Layout, cold: bool) {
        for (b, spec) in layout.blocks.iter().enumerate() {
            let live = &mut self.blocks[b];
            let name = &spec.config.name;
            let Some((f, j)) = spec.first else {
                if let Some(v) = live.pending.take() {
                    tracing::warn!(
                        "a retained value was not taken: {}: no program of this runtime declares the data block",
                        v.path
                    );
                }
                continue;
            };
            let kept = if cold {
                None
            } else {
                match &live.master {
                    Some(master) => spec.config.retain.then(|| RetainedValue {
                        path: name.clone(),
                        ty: spec.config.ty.clone(),
                        bytes: hex::encode(master),
                        layout: Some(spec.config.layout.clone()),
                    }),
                    None => live.pending.take(),
                }
            };
            live.pending = None;
            let mut master = self.programs[f].image[layout.copy_range(f, j)].to_vec();
            if let Some(v) = kept {
                let why = if !spec.config.retain {
                    Some(format!("{name}: the data block is not RETAIN"))
                } else if v.layout.as_deref() != Some(spec.config.layout.as_str()) {
                    Some(format!(
                        "{name}: its layout is {} now, not {}",
                        spec.config.layout,
                        v.layout.as_deref().unwrap_or("")
                    ))
                } else {
                    match hex::decode(&v.bytes) {
                        Ok(bytes) if bytes.len() == master.len() => {
                            master.copy_from_slice(&bytes);
                            None
                        }
                        _ => Some(format!(
                            "{name}: the value kept is not {} bytes",
                            spec.config.size
                        )),
                    }
                };
                if let Some(why) = why {
                    tracing::warn!("a retained value was not taken: {why}");
                }
            }
            live.master = Some(master);
        }
        self.spread(layout);
    }

    /// Every program's copy of each data block, in its image, set whole from the block's
    /// master.
    pub fn spread(&mut self, layout: &Layout) {
        for (b, spec) in layout.blocks.iter().enumerate() {
            let Some(master) = &self.blocks[b].master else {
                continue;
            };
            for &(p, j) in &spec.uses {
                self.programs[p].image[layout.copy_range(p, j)].copy_from_slice(master);
            }
        }
    }

    /// Each data block's master made again from the images (an online change, after the
    /// running values were carried into them): each member from its writer's copy, a member no
    /// program writes from the first declaring program's copy.
    pub fn compose(&mut self, layout: &Layout) {
        for (b, spec) in layout.blocks.iter().enumerate() {
            let (Some(first), Some(master)) = (spec.first, self.blocks[b].master.as_mut()) else {
                continue;
            };
            for (i, writer) in spec.writer.iter().enumerate() {
                let (p, j) = writer.unwrap_or(first);
                master[layout.master_range(b, i)]
                    .copy_from_slice(&self.programs[p].image[layout.member_range(p, j, i)]);
            }
        }
    }

    /// What the host keeps of the data blocks: each RETAIN block's master, or before the first
    /// RUN the value the host gave for any block, so a runtime loaded in STOP never has a kept
    /// value replaced or lost.
    pub fn kept_blocks(&self, layout: &Layout) -> Vec<RetainedValue> {
        let mut values = Vec::new();
        for (spec, live) in layout.blocks.iter().zip(&self.blocks) {
            match (&live.master, &live.pending) {
                (Some(master), _) if spec.config.retain => values.push(RetainedValue {
                    path: spec.config.name.clone(),
                    ty: spec.config.ty.clone(),
                    bytes: hex::encode(master),
                    layout: Some(spec.config.layout.clone()),
                }),
                (None, Some(pending)) => values.push(pending.clone()),
                _ => {}
            }
        }
        values
    }

    /// Program `p` begins its run in a task run: each member of each block it declares that
    /// it does not write is copied into its working memory, from the working memory of the
    /// member's writer when that is a program of the same task that has already ended its run
    /// in this task run, otherwise from the master. Then its forces are held, so a force wins.
    fn hand_in(&self, layout: &Layout, p: usize, work: &mut [Work]) {
        if !layout.declares(p) {
            return;
        }
        let (task, at) = layout.place[p];
        for (j, &b) in layout.block_of[p].iter().enumerate() {
            let spec = &layout.blocks[b];
            let Some(master) = &self.blocks[b].master else {
                continue;
            };
            for (i, writer) in spec.writer.iter().enumerate() {
                let to = layout.member_range(p, j, i);
                match *writer {
                    Some((w, _)) if w == p => {}
                    Some((w, wj)) if layout.place[w].0 == task && layout.place[w].1 < at => {
                        let from = layout.member_range(w, wj, i);
                        let (dst, src) = two(work, p, w);
                        dst.memory[to].copy_from_slice(&src.memory[from]);
                    }
                    _ => work[p].memory[to].copy_from_slice(&master[layout.master_range(b, i)]),
                }
            }
        }
        hold(&self.programs[p].forces, &mut work[p].memory);
    }

    /// Task `k`'s run has ended: each of its programs, in order, holds its forces and copies
    /// the members it writes into the master.
    fn hand_out(&mut self, layout: &Layout, k: usize, work: &mut [Work]) {
        for &p in &layout.tasks[k].programs {
            if !layout.declares(p) {
                continue;
            }
            let memory = &mut work[p].memory;
            hold(&self.programs[p].forces, memory);
            for (j, &b) in layout.block_of[p].iter().enumerate() {
                let Some(master) = self.blocks[b].master.as_mut() else {
                    continue;
                };
                for (i, m) in layout.plans[p].program.data[j].members.iter().enumerate() {
                    if m.written {
                        master[layout.master_range(b, i)]
                            .copy_from_slice(&memory[layout.member_range(p, j, i)]);
                    }
                }
            }
        }
    }

    /// Put in STOP: every force released, the run in progress dropped.
    pub fn stop(&mut self) {
        self.mode = RuntimeMode::Stop;
        self.fault = None;
        for p in &mut self.programs {
            p.forces.clear();
        }
        self.changed();
    }

    /// Stopped by a fault: every force released, the runs in progress dropped. The core then
    /// writes every output's safe value.
    fn fail(&mut self, why: String) {
        tracing::error!("{why}");
        self.mode = RuntimeMode::Fault;
        for p in &mut self.programs {
            p.forces.clear();
        }
        self.fault = Some(why);
        self.changed();
    }

    /// An exchange delivered the tags: they are the inputs of the next latch, and an event
    /// task's tag may have risen.
    pub fn feed(&mut self, layout: &Layout, inputs: Vec<(String, Option<Value>, bool)>) {
        let now = Instant::now();
        // An event task's tag as this exchange delivered it, `Some` only when GOOD and a BOOL. A
        // tag this exchange left out is not taken from an earlier one: it is not known now.
        let levels: Vec<Option<bool>> = layout
            .tasks
            .iter()
            .map(|spec| {
                let tag = spec.event_tag.as_ref()?;
                match inputs.iter().rfind(|(t, _, _)| t == tag) {
                    Some((_, Some(v), true)) => match to_vm(ElemType::Bool, v) {
                        Some(VmValue::Bool(b)) => Some(b),
                        _ => None,
                    },
                    _ => None,
                }
            })
            .collect();
        for (tag, value, good) in inputs {
            self.inputs.insert(tag, (value, good));
        }
        if !self.fed {
            self.fed = true;
            // The scheduler starts the tasks at its next turn, now. What owes a run from then: a
            // cyclic task from its first due time, a cycle task and a startup task that runs at
            // once; an event task from its trigger.
            for (t, s) in self.tasks.iter_mut().zip(&self.sched) {
                t.owed_since = match s.kind() {
                    Kind::Cyclic { phase, .. } => Some(now + Duration::from_nanos(phase as u64)),
                    Kind::Cycle { .. } | Kind::Startup => Some(now),
                    Kind::Event => None,
                };
            }
        }
        if self.mode != RuntimeMode::Run {
            return;
        }
        for ((t, spec), level) in self.tasks.iter_mut().zip(&layout.tasks).zip(levels) {
            if spec.event_tag.is_none() {
                continue;
            }
            // A rising edge is a GOOD FALSE then a GOOD TRUE: none at the first exchange, none
            // across a time the tag was not GOOD or not delivered. In doubt the task does not
            // run.
            if t.prev == Some(false) && level == Some(true) {
                if t.triggered {
                    t.missed += 1;
                } else {
                    t.triggered = true;
                }
                t.owed_since.get_or_insert(now);
            }
            t.prev = level;
        }
    }

    /// Stop the runtime for the watchdog, when it is due: in RUN, fed, and a task has owed a
    /// run for longer than its limit, or the scheduler thread has ended.
    pub fn watchdog(&mut self, layout: &Layout, thread_ended: bool, now: Instant) {
        if self.mode != RuntimeMode::Run || !self.fed {
            return;
        }
        let why = if thread_ended {
            Some("the scan thread has ended; load the program again".to_string())
        } else {
            self.tasks
                .iter()
                .zip(&layout.tasks)
                .find_map(|(t, spec)| {
                    let quiet = now.saturating_duration_since(t.owed_since?);
                    (quiet > spec.watchdog).then_some((quiet, spec))
                })
                .map(|(quiet, spec)| {
                    if layout.implied {
                        format!(
                            "no scan completed for {} ms, past the watchdog's {} ms",
                            quiet.as_millis(),
                            spec.watchdog.as_millis()
                        )
                    } else {
                        format!(
                            "task '{}' has ended no run for {} ms, past its watchdog's {} ms",
                            spec.config.name,
                            quiet.as_millis(),
                            spec.watchdog.as_millis()
                        )
                    }
                })
        };
        if let Some(why) = why {
            self.fail(format!("watchdog: {why}"));
        }
    }

    /// Why program `p` reports nothing, when it does not.
    pub fn silent_because(&self, layout: &Layout, p: usize) -> Option<String> {
        match self.mode {
            RuntimeMode::Stop => Some("the program is stopped".to_string()),
            RuntimeMode::Fault => Some(format!(
                "the program faulted: {}",
                self.fault.as_deref().unwrap_or("no reason recorded")
            )),
            RuntimeMode::Run if !self.programs[p].completed => Some(if layout.implied {
                "the program has not completed a scan yet".to_string()
            } else {
                "the program has not completed a run yet".to_string()
            }),
            RuntimeMode::Run => None,
        }
    }

    fn task_state(&self, layout: &Layout, k: usize, now: i64) -> TaskState {
        let spec = &layout.tasks[k];
        let c = self.sched[k].counters();
        TaskState {
            name: spec.config.name.clone(),
            kind: spec.config.kind.clone(),
            priority: spec.config.priority,
            programs: spec.config.programs.clone(),
            watchdog_ms: spec.watchdog.as_millis().min(u64::MAX as u128) as u64,
            runs: c.runs,
            overruns: c.overruns.saturating_add(self.tasks[k].missed),
            preemptions: c.preemptions,
            last_run_us: c.last_run.map(us_of_ns),
            longest_run_us: c.longest_run.map(us_of_ns),
            run_age_us: c.last_end.map(|end| us_of_ns(now.saturating_sub(end))),
        }
    }

    /// What the runtime is doing. Given no tasks, exactly what it said before there were tasks;
    /// with tasks, each task's state, and above it the one task's figures, or for several their
    /// sums, the longest of their runs, the last run of the task that ended last, and no age:
    /// no one age says how current the values of several tasks are.
    pub fn state(&self, layout: &Layout) -> RuntimeState {
        let now = clock(self.since);
        let tasks: Vec<TaskState> = (0..layout.tasks.len())
            .map(|k| self.task_state(layout, k, now))
            .collect();
        let forced = self
            .programs
            .iter()
            .flat_map(|p| &p.forces)
            .map(|f| ForcedValue {
                variable: f.variable.clone(),
                value: crate::to_json(f.value),
            })
            .collect();
        let join =
            |of: fn(&Plan) -> &str| layout.plans.iter().map(of).collect::<Vec<_>>().join(", ");
        let scans = tasks.iter().map(|t| t.runs).sum();
        let overruns = tasks.iter().map(|t| t.overruns).sum();
        let max_scan_us = tasks
            .iter()
            .filter_map(|t| t.longest_run_us)
            .max()
            .unwrap_or(0);
        // The task that ended a run last.
        let last = (0..tasks.len())
            .filter_map(|k| self.sched[k].counters().last_end.map(|end| (end, k)))
            .max()
            .map(|(_, k)| k);
        let last_scan_us = last.and_then(|k| tasks[k].last_run_us).unwrap_or(0);
        let scan_age_us = match tasks.as_slice() {
            [one] => one.run_age_us,
            _ => None,
        };
        let (tasks, programs) = if layout.implied {
            (Vec::new(), Vec::new())
        } else {
            (
                tasks,
                layout
                    .plans
                    .iter()
                    .map(|p| ProgramVersion {
                        name: p.program.name.clone(),
                        version: p.program.version.clone(),
                    })
                    .collect(),
            )
        };
        RuntimeState {
            mode: self.mode,
            fault: self.fault.clone(),
            program: join(|p| &p.program.name),
            version: join(|p| &p.program.version),
            scans,
            last_scan_us,
            max_scan_us,
            overruns,
            scan_age_us,
            forced,
            tasks,
            programs,
        }
    }

    /// The triggers the exchanges saw since the last turn, given to the scheduler at `now`; a
    /// trigger for a task already waiting to run makes no run and is counted.
    fn take_triggers(&mut self, now: i64) {
        for k in 0..self.tasks.len() {
            if std::mem::take(&mut self.tasks[k].triggered) {
                if self.sched[k].is_ready() {
                    self.tasks[k].missed += 1;
                } else {
                    sched::trigger(&mut self.sched, k, now);
                }
            }
        }
    }

    /// The beginning of a run of a task: program `p`'s inputs from the last exchange, then its
    /// setpoints, then its forces, into its working memory.
    fn latch(&mut self, plan: &Plan, p: usize, memory: &mut [u8]) {
        for (var, tag, of) in &plan.inputs {
            let Some((value, good)) = self.inputs.get(tag) else {
                continue;
            };
            match of {
                InputOf::Good => {
                    write(memory, var.offset, VmValue::Bool(*good));
                }
                InputOf::Value => {
                    if let (true, Some(v)) = (*good, value.as_ref().and_then(|v| to_vm(var.ty, v)))
                    {
                        write(memory, var.offset, v);
                    }
                }
            }
        }
        let live = &mut self.programs[p];
        for (offset, value) in live.setpoints.drain(..) {
            write(memory, offset, value);
        }
        hold(&live.forces, memory);
    }

    /// The end of a run of task `k` at `now`: its programs' forces written again, their working
    /// memory copied to their images, the setpoints written since put back over them.
    fn finish(&mut self, layout: &Layout, k: usize, work: &mut [Work], now: i64) {
        // The master and the images change together: a reader of another task sees this
        // task's run whole or not at all.
        self.hand_out(layout, k, work);
        for &p in &layout.tasks[k].programs {
            let live = &mut self.programs[p];
            let memory = &mut work[p].memory;
            hold(&live.forces, memory);
            live.image.copy_from_slice(memory);
            for (offset, value) in &live.setpoints {
                write(&mut live.image, *offset, *value);
            }
            live.completed = true;
        }
        sched::ended(&mut self.sched, k, now);
        // Ready again, or triggered by an exchange since the scheduler last took the triggers:
        // the scheduler gives that trigger at its next turn, and the run it makes is owed now.
        let again = self.sched[k].is_ready() || self.tasks[k].triggered;
        let owed = owed_after(
            self.sched[k].kind(),
            self.started_at,
            now,
            self.sched[k].counters().last_run.unwrap_or(0),
            again,
        );
        self.tasks[k].owed_since =
            owed.map(|ns| self.since + Duration::from_nanos(ns.max(0) as u64));
    }
}

/// When a task whose run ended at `end` owes its next run, on the runtime's clock: a cyclic
/// task at its next due time (`start + phase + k * interval`, the first at or after the end), a
/// cycle task when it may begin again, an event task triggered again while it ran at once; a
/// startup task, or an event task not triggered, owes none. So a watchdog shorter than a
/// task's interval or its phase limits how long a run takes once due, and never faults a task
/// that is waiting for its time.
fn owed_after(kind: Kind, start: i64, end: i64, last_run: i64, ready: bool) -> Option<i64> {
    match kind {
        Kind::Cyclic { interval, phase } => {
            let first = start.saturating_add(phase);
            if end <= first {
                return Some(first);
            }
            // `interval` is above zero (`sched::Task::new`).
            let since_first = end.abs_diff(first);
            let k = since_first.div_ceil(interval.unsigned_abs());
            Some(first.saturating_add_unsigned(k.saturating_mul(interval.unsigned_abs())))
        }
        Kind::Cycle { min_cycle } => {
            Some(end.max(end.saturating_sub(last_run).saturating_add(min_cycle)))
        }
        Kind::Event if ready => Some(end),
        Kind::Event | Kind::Startup => None,
    }
}

/// Program `a`'s working space to write and program `b`'s to read; `a` is not `b`.
fn two(work: &mut [Work], a: usize, b: usize) -> (&mut Work, &Work) {
    if a < b {
        let (left, right) = work.split_at_mut(b);
        (&mut left[a], &right[0])
    } else {
        let (left, right) = work.split_at_mut(a);
        (&mut right[0], &left[b])
    }
}

/// A program's working space on the scheduler thread.
struct Work {
    memory: Vec<u8>,
    stack: Vec<u64>,
    calls: Vec<Frame>,
}

/// A task's run in progress: which of its programs runs, and how far it is.
#[derive(Clone, Copy)]
struct TaskRun {
    at: usize,
    run: Run,
}

/// What the scheduler does in one turn.
enum Turn {
    /// Nothing to run: wait for so long, or until woken.
    Wait(Duration),
    /// Run a piece of this task's run.
    Piece(usize),
}

/// The scheduler thread: until `quit`, runs the tasks of `layout` while the runtime is in RUN
/// and has been fed.
pub(crate) fn schedule(layout: Arc<Layout>, control: Arc<Control>, quit: Arc<AtomicBool>) {
    let programs = match layout
        .plans
        .iter()
        .map(|p| Program::load(&p.bytes))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(programs) => programs,
        Err(_) => {
            control.lock().fail("the program does not load".to_string());
            return;
        }
    };
    let mut work: Vec<Work> = programs
        .iter()
        .map(|p| Work {
            memory: vec![0u8; p.memory_size()],
            stack: vec![0u64; p.stack_depth().max(1)],
            calls: vec![Frame::default(); p.call_depth()],
        })
        .collect();
    let mut runs: Vec<Option<TaskRun>> = vec![None; layout.tasks.len()];
    let mut seen: Option<u64> = None;
    let mut since = Instant::now();
    while !quit.load(Ordering::SeqCst) {
        let turn = {
            let mut guard = control.lock();
            let g = &mut *guard;
            if seen != Some(g.generation) {
                // A change of mode, or a fault: what was running is dropped, and in RUN every
                // program goes on from its image.
                seen = Some(g.generation);
                runs.iter_mut().for_each(|r| *r = None);
                since = g.since;
                if g.mode == RuntimeMode::Run {
                    for (w, live) in work.iter_mut().zip(&g.programs) {
                        w.memory.copy_from_slice(&live.image);
                    }
                }
            }
            let turn = if g.mode != RuntimeMode::Run || !g.fed {
                Turn::Wait(IDLE_WAIT)
            } else {
                let now = clock(since);
                if !g.started {
                    // At the first exchange after RUN: no run before the first inputs.
                    sched::start(&mut g.sched, now);
                    g.started = true;
                    g.started_at = now;
                }
                g.take_triggers(now);
                match sched::next(&mut g.sched, now) {
                    Next::Idle => {
                        let until = sched::wake(&g.sched)
                            .map(|w| Duration::from_nanos(w.saturating_sub(now).max(0) as u64))
                            .unwrap_or(IDLE_WAIT);
                        Turn::Wait(until.min(IDLE_WAIT))
                    }
                    Next::Begin(k) => {
                        for &p in &layout.tasks[k].programs {
                            g.latch(&layout.plans[p], p, &mut work[p].memory);
                        }
                        let first = layout.tasks[k].programs[0];
                        g.hand_in(&layout, first, &mut work);
                        runs[k] = Some(TaskRun {
                            at: 0,
                            run: Run::begin(&programs[first], clock(since), BUDGET),
                        });
                        Turn::Piece(k)
                    }
                    Next::Resume(k) => Turn::Piece(k),
                }
            };
            match turn {
                Turn::Piece(k) => Some(k),
                Turn::Wait(d) => {
                    // Woken early by an exchange, a change of mode or a load.
                    if !d.is_zero() {
                        drop(control.wake.wait_timeout(guard, d));
                    }
                    None
                }
            }
        };
        let Some(k) = turn else {
            continue;
        };
        let spec = &layout.tasks[k];
        let Some(task_run) = runs[k].as_mut() else {
            // In progress for the scheduler with nothing to continue: cannot happen, since
            // both are dropped together; end it rather than spin on it.
            sched::ended(&mut control.lock().sched, k, clock(since));
            continue;
        };
        let p = spec.programs[task_run.at];
        let w = &mut work[p];
        match task_run.run.resume(
            &programs[p],
            &mut w.memory,
            &mut w.stack,
            &mut w.calls,
            PIECE,
        ) {
            Ok(Piece::Suspended) => {}
            Ok(Piece::Ended(_)) if task_run.at + 1 < spec.programs.len() => {
                let next = spec.programs[task_run.at + 1];
                // A runtime of today takes no lock here.
                if layout.declares(p) || layout.declares(next) {
                    let g = control.lock();
                    if Some(g.generation) != seen {
                        // A change of mode or a fault during the piece: the task run is
                        // dropped, and nothing is handed over.
                        runs[k] = None;
                        continue;
                    }
                    // So a forced value is what a later program of the task takes.
                    hold(&g.programs[p].forces, &mut work[p].memory);
                    g.hand_in(&layout, next, &mut work);
                }
                task_run.at += 1;
                task_run.run = Run::begin(&programs[next], clock(since), BUDGET);
            }
            Ok(Piece::Ended(_)) => {
                runs[k] = None;
                let mut g = control.lock();
                if Some(g.generation) == seen {
                    g.finish(&layout, k, &mut work, clock(since));
                }
                drop(g);
                // A cycle task is ready again at once: let an exchange take the lock first.
                std::thread::yield_now();
            }
            Err(fault) => {
                let mut g = control.lock();
                if Some(g.generation) == seen {
                    let n = g.sched[k].counters().runs.saturating_add(1);
                    let why = if layout.implied {
                        format!("scan {n} stopped: {}", describe(fault))
                    } else {
                        format!(
                            "task '{}' program '{}' run {n} stopped: {}",
                            spec.config.name,
                            layout.plans[p].program.name,
                            describe(fault)
                        )
                    };
                    g.fail(why);
                }
                runs.iter_mut().for_each(|r| *r = None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A runtime of one program in its implied task, in RUN, fed, with a run completed and one
    /// variable forced.
    fn running(implied: bool) -> (Layout, Shared) {
        let bytes = include_bytes!(
            "../../../crates/mythos-logic-compiler/tests/fixtures/format1/integers_and_control.mlbc"
        );
        let program = json!({
            "name": "P", "version": "1", "interval_ms": 10,
            "code": hex::encode(bytes), "symbols": [],
        });
        let config = if implied {
            json!({ "program": program })
        } else {
            json!({ "programs": [program], "tasks": [{"name": "main",
                "kind": {"type": "cyclic", "interval_ms": 10}, "priority": 0,
                "programs": ["P"]}] })
        };
        let layout = crate::plan(&config).ok().unwrap().layout;
        let mut shared = Shared::new(&layout, Vec::new());
        shared.restart(&layout, false).unwrap();
        shared.feed(&layout, Vec::new());
        shared.programs[0].forces.push(Forced {
            variable: "x".into(),
            offset: 0,
            value: VmValue::Int(1),
        });
        (layout, shared)
    }

    #[test]
    fn the_watchdog_faults_a_running_program_that_completes_no_scan_and_only_that() {
        let now = Instant::now();
        let long_ago = now - Duration::from_millis(2_500);
        let limit = Duration::from_secs(2);

        let (layout, mut quiet) = running(true);
        assert_eq!(layout.tasks[0].watchdog, limit);
        quiet.tasks[0].owed_since = Some(long_ago);
        quiet.watchdog(&layout, false, now);
        assert_eq!(quiet.mode, RuntimeMode::Fault);
        assert!(
            quiet
                .fault
                .as_deref()
                .is_some_and(|f| f.starts_with("watchdog: no scan completed for 2500 ms")),
            "{:?}",
            quiet.fault
        );
        assert!(
            quiet.programs[0].forces.is_empty(),
            "a fault releases every force"
        );

        // A task written out says which task.
        let (layout, mut named) = running(false);
        named.tasks[0].owed_since = Some(long_ago);
        named.watchdog(&layout, false, now);
        assert_eq!(
            named.fault.as_deref(),
            Some("watchdog: task 'main' has ended no run for 2500 ms, past its watchdog's 2000 ms")
        );

        let (layout, mut ended) = running(true);
        ended.watchdog(&layout, true, now);
        assert_eq!(ended.mode, RuntimeMode::Fault);
        assert_eq!(
            ended.fault.as_deref(),
            Some("watchdog: the scan thread has ended; load the program again")
        );

        // Scanning, not yet fed, or not running: left as it is.
        for (mode, fed, owed, thread_ended) in [
            (
                RuntimeMode::Run,
                true,
                Some(now - Duration::from_millis(1_900)),
                false,
            ),
            (RuntimeMode::Run, false, None, true),
            (RuntimeMode::Stop, true, Some(long_ago), true),
            (RuntimeMode::Fault, true, Some(long_ago), true),
        ] {
            let (layout, mut s) = running(true);
            s.mode = mode;
            s.fed = fed;
            s.tasks[0].owed_since = owed;
            s.watchdog(&layout, thread_ended, now);
            assert_eq!(s.mode, mode);
            assert_eq!(s.programs[0].forces.len(), 1);
        }
    }

    const GO: &str = "external.A.B.go";

    /// One event task `edge` at priority 5, running program `ev`, which reads GO into a BOOL.
    fn event_task() -> (Layout, Vec<Work>) {
        let c = mythos_logic_compiler::compile(
            "PROGRAM ev\nVAR trig : BOOL; n : DINT; END_VAR\n  n := n + 1;\nEND_PROGRAM\n",
            &mythos_logic_compiler::TargetProfile::pc(),
        )
        .unwrap_or_else(|d| panic!("{d:?}"));
        let config = json!({
            "programs": [{"name": "ev", "version": "1", "interval_ms": 10,
                "code": hex::encode(&c.bytecode), "symbols": c.symbols,
                "inputs": [{"variable": "trig", "tag": GO}]}],
            "tasks": [{"name": "edge", "kind": {"type": "event", "tag": GO},
                "priority": 5, "programs": ["ev"]}]
        });
        let layout = crate::plan(&config).ok().unwrap().layout;
        let work = layout
            .plans
            .iter()
            .map(|p| Work {
                memory: vec![0u8; p.memory_size],
                stack: Vec::new(),
                calls: Vec::new(),
            })
            .collect();
        (layout, work)
    }

    fn go(level: bool) -> Vec<(String, Option<Value>, bool)> {
        vec![(GO.to_string(), Some(json!(level)), true)]
    }

    /// An event task in RUN, fed GO FALSE, started, then triggered by a rising edge and begun.
    fn an_event_run_begun(layout: &Layout) -> Shared {
        let mut g = Shared::new(layout, Vec::new());
        g.restart(layout, false).unwrap();
        g.feed(layout, go(false));
        sched::start(&mut g.sched, 0);
        g.started = true;
        g.feed(layout, go(true));
        g.take_triggers(1_000);
        assert_eq!(sched::next(&mut g.sched, 1_000), Next::Begin(0));
        g
    }

    #[test]
    fn an_edge_seen_during_the_last_piece_of_an_event_run_is_watched_until_its_run() {
        let (layout, mut work) = event_task();
        let mut g = an_event_run_begun(&layout);
        // While the run's last piece runs with no lock held, two exchanges: FALSE, then TRUE.
        g.feed(&layout, go(false));
        g.feed(&layout, go(true));
        assert!(g.tasks[0].triggered, "the edge was seen");
        // The scheduler finishes the run before its next turn takes the trigger.
        g.finish(&layout, 0, &mut work, 2_000);
        assert!(g.tasks[0].owed_since.is_some(), "the triggered run is owed");
        g.take_triggers(3_000);
        assert!(g.sched[0].is_ready(), "the task owes a run");
        // Starved: never begun. Ten seconds on, past its 2 s watchdog, it faults.
        g.watchdog(&layout, false, Instant::now() + Duration::from_secs(10));
        assert_eq!(
            g.mode,
            RuntimeMode::Fault,
            "owed_since = {:?}",
            g.tasks[0].owed_since
        );
        assert!(
            g.fault
                .as_deref()
                .is_some_and(|f| f.starts_with("watchdog: task 'edge' has ended no run for")),
            "{:?}",
            g.fault
        );
    }

    #[test]
    fn an_edge_taken_by_the_scheduler_during_an_event_run_is_watched_until_its_run() {
        // The control: a turn of the scheduler between the edge and the end of the run.
        let (layout, mut work) = event_task();
        let mut g = an_event_run_begun(&layout);
        g.feed(&layout, go(false));
        g.feed(&layout, go(true));
        g.take_triggers(1_500);
        g.finish(&layout, 0, &mut work, 2_000);
        assert!(g.sched[0].is_ready());
        g.watchdog(&layout, false, Instant::now() + Duration::from_secs(10));
        assert_eq!(g.mode, RuntimeMode::Fault);
        // And a run that ends with no edge since owes nothing.
        let (layout, mut work) = event_task();
        let mut g = an_event_run_begun(&layout);
        g.finish(&layout, 0, &mut work, 2_000);
        assert_eq!(g.tasks[0].owed_since, None);
        g.watchdog(&layout, false, Instant::now() + Duration::from_secs(10));
        assert_eq!(g.mode, RuntimeMode::Run);
    }

    /// Programs A and B in task `t`, in that order, and C in task `u`, each declaring block
    /// `S` of members `n` (A writes it) and `m` (B writes it); C writes neither.
    fn three_sharing() -> (Layout, Vec<Work>) {
        use mythos_logic_compiler::{check_project, compile_sources, DataBlock, Sources};
        let types = "TYPE T : STRUCT n : DINT; m : DINT; END_STRUCT; END_TYPE";
        let library = [("types.st", types)];
        let blocks = [DataBlock { name: "S", ty: "T" }];
        let program = |source: &str| {
            let c = compile_sources(
                &Sources {
                    st: Some(("main.st", source)),
                    library: &library,
                    data: &blocks,
                    ..Default::default()
                },
                &mythos_logic_compiler::TargetProfile::pc(),
            )
            .unwrap_or_else(|d| panic!("{d:?}"));
            json!({"name": c.name, "version": "1", "interval_ms": 10,
                "code": hex::encode(&c.bytecode), "symbols": c.symbols, "data": c.data})
        };
        let l = check_project(&Sources {
            library: &library,
            data: &blocks,
            ..Default::default()
        })
        .unwrap_or_else(|d| panic!("{d:?}"))
        .remove(0);
        let members: Vec<Value> = l
            .members
            .iter()
            .map(|m| json!({"name": m.name, "type": m.ty, "offset": m.offset, "size": m.size}))
            .collect();
        let config = json!({
            "data": [{"name": l.name, "type": l.ty, "layout": l.layout, "size": l.size,
                      "members": members}],
            "programs": [
                program("PROGRAM A VAR_EXTERNAL S : T; END_VAR S.n := 1; END_PROGRAM"),
                program("PROGRAM B VAR x : DINT; END_VAR VAR_EXTERNAL S : T; END_VAR S.m := 1; END_PROGRAM"),
                program("PROGRAM C VAR y : DINT; z : DINT; END_VAR VAR_EXTERNAL S : T; END_VAR y := S.n + S.m; END_PROGRAM"),
            ],
            "tasks": [
                {"name": "t", "kind": {"type": "cyclic", "interval_ms": 10}, "priority": 1,
                 "programs": ["A", "B"]},
                {"name": "u", "kind": {"type": "cyclic", "interval_ms": 10}, "priority": 2,
                 "programs": ["C"]},
            ],
        });
        let layout = crate::plan(&config)
            .map_err(|e| e.to_string())
            .unwrap()
            .layout;
        let work = layout
            .plans
            .iter()
            .map(|p| Work {
                memory: vec![0u8; p.memory_size],
                stack: Vec::new(),
                calls: Vec::new(),
            })
            .collect();
        (layout, work)
    }

    #[test]
    fn a_program_takes_in_only_what_it_does_not_write_and_the_task_hands_out_at_its_end() {
        let (layout, mut work) = three_sharing();
        let (a, b, c) = (0, 1, 2);
        assert_eq!(layout.blocks[0].writer, [Some((a, 0)), Some((b, 0))]);
        let mut g = Shared::new(&layout, Vec::new());
        g.restart(&layout, true).unwrap();
        let get = |work: &[Work], p: usize, i: usize| -> Vec<u8> {
            work[p].memory[layout.member_range(p, 0, i)].to_vec()
        };
        let set = |work: &mut [Work], p: usize, i: usize, v: i32| {
            work[p].memory[layout.member_range(p, 0, i)].copy_from_slice(&v.to_le_bytes());
        };
        // The master: n = 1, m = 2. Every working memory holds other values.
        g.blocks[0]
            .master
            .as_mut()
            .unwrap()
            .copy_from_slice(&[1i32.to_le_bytes(), 2i32.to_le_bytes()].concat());
        for p in [a, b, c] {
            set(&mut work, p, 0, 100 + p as i32);
            set(&mut work, p, 1, 200 + p as i32);
        }
        // A begins: it keeps its own n; m it takes from the master (B, its writer, comes
        // later in the task).
        g.hand_in(&layout, a, &mut work);
        assert_eq!(get(&work, a, 0), 100i32.to_le_bytes());
        assert_eq!(get(&work, a, 1), 2i32.to_le_bytes());
        // A ends, B begins: n from A's working memory, its own m kept.
        g.hand_in(&layout, b, &mut work);
        assert_eq!(get(&work, b, 0), 100i32.to_le_bytes());
        assert_eq!(get(&work, b, 1), 201i32.to_le_bytes());
        // Nothing has reached the master yet. C, of another task, takes both from it.
        assert_eq!(
            g.blocks[0].master.as_deref().unwrap(),
            &[1i32.to_le_bytes(), 2i32.to_le_bytes()].concat()[..]
        );
        g.hand_in(&layout, c, &mut work);
        assert_eq!(get(&work, c, 0), 1i32.to_le_bytes());
        assert_eq!(get(&work, c, 1), 2i32.to_le_bytes());
        // Task t ends: each of its programs hands out what it writes, and only that.
        g.hand_out(&layout, 0, &mut work);
        assert_eq!(
            g.blocks[0].master.as_deref().unwrap(),
            &[100i32.to_le_bytes(), 201i32.to_le_bytes()].concat()[..]
        );
        // Task u's end hands out nothing: C writes no member.
        set(&mut work, c, 0, -1);
        g.hand_out(&layout, 1, &mut work);
        assert_eq!(
            g.blocks[0].master.as_deref().unwrap(),
            &[100i32.to_le_bytes(), 201i32.to_le_bytes()].concat()[..]
        );
    }

    #[test]
    fn a_task_owes_its_next_run_from_when_it_is_due() {
        let cyclic = Kind::Cyclic {
            interval: 100,
            phase: 1_000,
        };
        // Started at 50: due at 1050, 1150, 1250, …
        assert_eq!(
            owed_after(cyclic, 50, 10, 5, false),
            Some(1_050),
            "before its phase"
        );
        assert_eq!(
            owed_after(cyclic, 50, 1_050, 5, false),
            Some(1_050),
            "ended on a due time"
        );
        assert_eq!(owed_after(cyclic, 50, 1_051, 5, false), Some(1_150));
        assert_eq!(owed_after(cyclic, 50, 1_150, 5, false), Some(1_150));
        assert_eq!(owed_after(cyclic, 50, 1_420, 5, false), Some(1_450));
        // A cycle task: at its end, or its minimum cycle after it began, whichever is later.
        let cycle = Kind::Cycle { min_cycle: 300 };
        assert_eq!(owed_after(cycle, 0, 1_000, 100, false), Some(1_200));
        assert_eq!(owed_after(cycle, 0, 1_000, 500, false), Some(1_000));
        assert_eq!(
            owed_after(Kind::Event, 0, 7, 1, true),
            Some(7),
            "triggered again"
        );
        assert_eq!(owed_after(Kind::Event, 0, 7, 1, false), None);
        assert_eq!(owed_after(Kind::Startup, 0, 7, 1, false), None);
    }

    #[test]
    fn the_state_says_how_long_ago_a_scan_completed_and_nothing_before_the_first() {
        let (layout, mut s) = running(true);
        // Fed, no run completed yet: no age, no time measured.
        let state = s.state(&layout);
        assert_eq!(state.scan_age_us, None);
        assert_eq!((state.scans, state.last_scan_us), (0, 0));

        // A run from 1 s to 1.5 s of the clock, read at 3 s: 1.5 s old.
        s.since = Instant::now() - Duration::from_secs(3);
        sched::start(&mut s.sched, 0);
        assert_eq!(sched::next(&mut s.sched, 1_000_000_000), Next::Begin(0));
        assert!(sched::ended(&mut s.sched, 0, 1_500_000_000));
        let state = s.state(&layout);
        let age = state.scan_age_us.expect("a scan has completed");
        assert!((1_500_000..2_500_000).contains(&age), "{age} µs");
        assert_eq!(
            (state.scans, state.last_scan_us, state.max_scan_us),
            (1, 500_000, 500_000)
        );
        assert!(state.tasks.is_empty() && state.programs.is_empty());
    }
}
