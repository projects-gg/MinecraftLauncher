//! Instances: MultiMC-style isolated game directories with their own version, loader, Java,
//! memory and launch settings.

pub mod defaults;
pub mod keys;
pub mod memory;
pub mod model;
pub mod options;
pub mod store;
pub mod usage;

pub use defaults::{
    DEFAULT_GUI_SCALE, DEFAULT_SERVER_ADDRESS, DEFAULT_SERVER_NAME, OptionsSeed, ServerSeed,
    add_servers, client_data_version, game_language, has_accessibility_onboarding,
    seed_default_server, seed_options,
};
pub use keys::{FirstKeys, KeyBinding, official_pack_bindings, official_pack_key_lines};
pub use memory::{
    AUTO_MAX_MB, MIN_HEAP_FLOOR_MB, MemoryIssue, MemoryLimits, MemoryValidation, auto_memory,
    auto_memory_at_least, clamp_memory, memory_limits, recommended_memory, system_memory_mb,
    validate_memory,
};
pub use model::{
    Hooks, INSTANCE_SCHEMA_VERSION, Instance, InstanceIcon, InstanceLoader, ManagedPack,
    MemorySettings, NewInstance, ServerAddress, WindowSettings,
};
pub use options::{
    GraphicsPreset, GraphicsState, apply_preset, detect_preset, graphics_state, preset_for_machine,
    set_graphics_preset, supports_presets, this_machine_preset,
};
pub use store::{GAME_DIR_NAME, INSTANCE_FILE, InstanceStore, slugify};
pub use usage::DiskUsage;
