//! # MythOS Native STruC++ Runtime Engine (`mythos-strucpp`)
//!
//! Provides in-controller compilation of IEC 61131-3 Structured Text to C++17 shared objects,
//! dynamic hot-reloading (< 50ms), safe C-ABI wrappers, SCHED_FIFO real-time scheduling,
//! and bidirectional TagBus synchronization.

pub mod compiler;
pub mod deploy;
pub mod error;
pub mod ffi;
pub mod hot_reload;
pub mod quality_gate;
pub mod runtime;
pub mod scheduler;
pub mod sync;

pub use compiler::{CompilationArtifacts, CompilerConfig, StrucppCompiler};
pub use deploy::{DeployRequest, DeployResult, LogicDeployer};
pub use error::StrucppError;
pub use ffi::{LocatedArea, LocatedSize, LocatedVarDescriptor, LocatedVarRaw, StrucppSymbols};
pub use hot_reload::{HotReloadManager, LoadedLogicModule};
pub use quality_gate::{
    ClearOutcome, DeviceExecutionMode, QualityGate, QualityLatch, QualityVisibility,
    ShmQualityPublisher,
};
pub use runtime::{RuntimeMetrics, StrucppRuntime};
pub use scheduler::{StagedSchedulerConfig, TaskScheduleConfig};
pub use sync::{TagBinding, TagSynchronizer};
