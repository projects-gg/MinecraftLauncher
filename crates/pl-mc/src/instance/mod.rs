//! Instances: MultiMC-style isolated game directories with their own version, loader, Java,
//! memory and launch settings.

pub mod defaults;
pub mod ias;
pub mod keys;
pub mod memory;
pub mod mod_configs;
pub mod model;
pub mod official;
pub mod options;
pub mod packs;
pub mod shaders;
pub mod store;
pub mod usage;

pub use defaults::{
    DEFAULT_GUI_SCALE, DEFAULT_SERVER_ADDRESS, DEFAULT_SERVER_NAME, OptionsSeed, ServerSeed,
    add_servers, client_data_version, game_language, has_accessibility_onboarding,
    seed_default_server, seed_options,
};
pub use keys::{
    FirstKeys, KeyBinding, KeyClash, KeyFix, KeyFixPlan, find_key_clashes, find_unbound_keys,
    fix_key_clashes_in, official_pack_bindings, official_pack_key_lines, plan_key_fix_in,
    undo_key_fix_in,
};
pub use memory::{
    AUTO_MAX_MB, GAME_OVERHEAD_MB, MIN_HEAP_FLOOR_MB, MemoryIssue, MemoryLimits, MemoryValidation,
    auto_memory, auto_memory_at_least, available_memory_mb, clamp_memory, fits_another_game,
    memory_limits, recommended_memory, system_memory_mb, validate_memory,
};
pub use mod_configs::{
    ModConfig, configs_for, fix_mod_settings_once, mod_configs, seed_mod_configs,
};
pub use model::{
    Hooks, INSTANCE_SCHEMA_VERSION, Instance, InstanceIcon, InstanceLoader, ManagedPack,
    MemorySettings, NewInstance, ServerAddress, WindowSettings,
};
pub use official::{
    OFFICIAL_CHOICE_FILE, OFFICIAL_INSTANCE_NAME, OfficialChoice, OfficialEnsure, find_official,
};
pub use options::{
    GraphicsPreset, GraphicsState, GraphicsSwitch, apply_preset, detect_preset, graphics_state,
    picked_preset_file, preset_for_machine, set_graphics_preset, supports_presets,
    switch_graphics_preset, this_machine_preset, undo_graphics_switch,
};
pub use packs::{
    IRIS_PROPERTIES, PackListChange, choose_shader_once, disable_resource_packs,
    enable_resource_packs, with_shader_choice,
};
pub use shaders::{ShaderState, read_shader_choice, set_shaders, shader_state};
pub use store::{GAME_DIR_NAME, INSTANCE_FILE, InstanceStore, slugify};
pub use usage::DiskUsage;
