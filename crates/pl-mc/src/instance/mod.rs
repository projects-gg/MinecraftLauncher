//! Instances: MultiMC-style isolated game directories with their own version, loader, Java,
//! memory and launch settings.

pub mod memory;
pub mod model;
pub mod store;

pub use memory::{
    MIN_HEAP_FLOOR_MB, MemoryIssue, MemoryLimits, MemoryValidation, clamp_memory, memory_limits,
    recommended_memory, system_memory_mb, validate_memory,
};
pub use model::{
    Hooks, INSTANCE_SCHEMA_VERSION, Instance, InstanceIcon, InstanceLoader, ManagedPack,
    MemorySettings, NewInstance, ServerAddress, WindowSettings,
};
pub use store::{GAME_DIR_NAME, INSTANCE_FILE, InstanceStore, slugify};
