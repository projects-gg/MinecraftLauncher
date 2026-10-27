//! Memory (RAM) helpers: system memory, recommended defaults and validation that mirrors the
//! legacy launcher's rules:
//! - minimum ≥ 1024 MB and ≤ half of the RAM,
//! - maximum ≥ minimum and ≤ RAM − 1 GB,
//! - a warning when the maximum is within 512 MB of that ceiling.
//!
//! RAM is rounded up to whole gigabytes first, like the legacy launcher did.

use serde::{Deserialize, Serialize};

use crate::instance::model::MemorySettings;

/// Lowest accepted minimum heap.
pub const MIN_HEAP_FLOOR_MB: u32 = 1024;
/// Margin below the ceiling that triggers the "high usage" warning.
const HIGH_USAGE_MARGIN_MB: u32 = 512;

/// Total physical memory in MB.
pub fn system_memory_mb() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.total_memory() / (1024 * 1024)
}

/// Physical memory the system can hand out now, in MB (free and reclaimable cache: Windows'
/// "available").
pub fn available_memory_mb() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.available_memory() / (1024 * 1024)
}

/// What a game takes beside its heap: the JVM itself, loaded classes, native buffers and the
/// graphics driver's share.
pub const GAME_OVERHEAD_MB: u64 = 1024;

/// Whether another game whose heap may grow to `max_heap_mb` fits in the memory the system can
/// hand out now (`available_mb`), next to the games already running (PLAY-AGAIN: small computers
/// freeze when two JVMs outgrow the RAM, so the player is told first).
pub fn fits_another_game(available_mb: u64, max_heap_mb: u32) -> bool {
    available_mb >= u64::from(max_heap_mb) + GAME_OVERHEAD_MB
}

/// Limits derived from the machine's RAM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryLimits {
    pub total_mb: u64,
    pub min_floor_mb: u32,
    /// Highest accepted minimum (half of the RAM).
    pub min_ceiling_mb: u32,
    /// Highest accepted maximum (RAM − 1 GB).
    pub max_ceiling_mb: u32,
    /// Maximums above this get a "high usage" warning.
    pub high_usage_mb: u32,
}

pub fn memory_limits(total_mb: u64) -> MemoryLimits {
    let gb = total_mb.div_ceil(1024).max(1);
    let to_u32 = |v: u64| u32::try_from(v).unwrap_or(u32::MAX);
    let max_ceiling = to_u32((gb * 1024).saturating_sub(1024));
    MemoryLimits {
        total_mb,
        min_floor_mb: MIN_HEAP_FLOOR_MB,
        min_ceiling_mb: to_u32(gb * 512),
        max_ceiling_mb: max_ceiling,
        high_usage_mb: max_ceiling.saturating_sub(HIGH_USAGE_MARGIN_MB),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MemoryIssue {
    MinTooLow {
        min_allowed_mb: u32,
    },
    MinTooHigh {
        max_allowed_mb: u32,
    },
    MaxBelowMin,
    MaxTooHigh {
        max_allowed_mb: u32,
    },
    /// Warning: the computer may slow down.
    HighUsage {
        threshold_mb: u32,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryValidation {
    pub errors: Vec<MemoryIssue>,
    pub warnings: Vec<MemoryIssue>,
}

impl MemoryValidation {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn validate_memory(settings: &MemorySettings, total_mb: u64) -> MemoryValidation {
    let limits = memory_limits(total_mb);
    let mut result = MemoryValidation::default();
    if settings.min_mb < limits.min_floor_mb {
        result.errors.push(MemoryIssue::MinTooLow {
            min_allowed_mb: limits.min_floor_mb,
        });
    } else if settings.min_mb > limits.min_ceiling_mb {
        result.errors.push(MemoryIssue::MinTooHigh {
            max_allowed_mb: limits.min_ceiling_mb,
        });
    }
    if settings.max_mb < settings.min_mb {
        result.errors.push(MemoryIssue::MaxBelowMin);
    }
    if settings.max_mb > limits.max_ceiling_mb {
        result.errors.push(MemoryIssue::MaxTooHigh {
            max_allowed_mb: limits.max_ceiling_mb,
        });
    } else if settings.max_mb > limits.high_usage_mb {
        result.warnings.push(MemoryIssue::HighUsage {
            threshold_mb: limits.high_usage_mb,
        });
    }
    result
}

/// Sensible defaults for this machine (more for modded instances), always within the limits
/// when the machine allows it.
pub fn recommended_memory(total_mb: u64, modded: bool) -> MemorySettings {
    let limits = memory_limits(total_mb);
    let gb = total_mb.div_ceil(1024);
    let wanted = match (modded, gb) {
        (false, 12..) => 4096,
        (false, 8..) => 3072,
        (false, _) => 2048,
        (true, 24..) => 8192,
        (true, 16..) => 6144,
        (true, 8..) => 4096,
        (true, 6..) => 3072,
        (true, _) => 2048,
    };
    let max_mb = wanted.min(limits.max_ceiling_mb).max(MIN_HEAP_FLOOR_MB);
    MemorySettings {
        min_mb: MIN_HEAP_FLOOR_MB.min(max_mb),
        max_mb,
    }
}

/// The most "Otomatik" ever gives the game: more rarely helps and lengthens garbage collection
/// pauses.
pub const AUTO_MAX_MB: u32 = 10 * 1024;

/// What "Otomatik" wants for an instance before the machine's limit: vanilla by RAM, modded by
/// the number of enabled mods.
fn auto_wanted_mb(total_gb: u64, modded: bool, mods: usize) -> u32 {
    if !modded {
        return match total_gb {
            16.. => 4096,
            8.. => 3072,
            _ => 2048,
        };
    }
    match mods {
        0..=15 => 3072,
        16..=50 => 4096,
        51..=100 => 5120,
        101..=160 => 6144,
        161..=250 => 8192,
        _ => AUTO_MAX_MB,
    }
}

/// How much the machine can give the game and still run the system, a browser and Discord:
/// half of the RAM up to 8 GB, the RAM minus 6 GB above that (never less than half).
fn auto_ceiling_mb(total_mb: u64) -> u32 {
    let gb = total_mb.div_ceil(1024).max(1);
    let ram = gb * 1024;
    let spare = if gb <= 8 {
        ram / 2
    } else {
        (ram - 6 * 1024).max(ram / 2)
    };
    let limits = memory_limits(total_mb);
    u32::try_from(spare)
        .unwrap_or(u32::MAX)
        .min(limits.max_ceiling_mb)
}

/// "Otomatik" memory: grows with the enabled mods (or the RAM, for vanilla) and stays within
/// what the machine can spare, in 512 MB steps, between 1 GB and [`AUTO_MAX_MB`].
pub fn auto_memory(total_mb: u64, modded: bool, enabled_mods: usize) -> MemorySettings {
    let gb = total_mb.div_ceil(1024).max(1);
    let wanted = auto_wanted_mb(gb, modded, enabled_mods);
    let max_mb =
        (wanted.min(auto_ceiling_mb(total_mb)) / 512 * 512).clamp(MIN_HEAP_FLOOR_MB, AUTO_MAX_MB);
    MemorySettings {
        min_mb: MIN_HEAP_FLOOR_MB.min(max_mb),
        max_mb,
    }
}

/// "Otomatik" memory raised to a modpack's own recommendation, as far as the machine can spare
/// (the pack's author knows what it needs; the cap still protects small machines).
pub fn auto_memory_at_least(
    memory: MemorySettings,
    wanted_mb: u32,
    total_mb: u64,
) -> MemorySettings {
    let reachable = wanted_mb.min(auto_ceiling_mb(total_mb)) / 512 * 512;
    let max_mb = memory.max_mb.max(reachable);
    MemorySettings {
        min_mb: memory.min_mb.min(max_mb),
        max_mb,
    }
}

/// Bring settings back inside the limits (what the legacy launcher did when a field was left
/// with an invalid value).
pub fn clamp_memory(settings: &MemorySettings, total_mb: u64) -> MemorySettings {
    let limits = memory_limits(total_mb);
    let min_mb = settings.min_mb.clamp(
        limits.min_floor_mb,
        limits.min_ceiling_mb.max(limits.min_floor_mb),
    );
    let max_mb = settings.max_mb.min(limits.max_ceiling_mb).max(min_mb);
    MemorySettings { min_mb, max_mb }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB16: u64 = 16 * 1024 - 120; // what sysinfo reports for a "16 GB" machine

    #[test]
    fn limits_follow_legacy_rules() {
        let l = memory_limits(GB16);
        assert_eq!(l.min_ceiling_mb, 8192);
        assert_eq!(l.max_ceiling_mb, 15360);
        assert_eq!(l.high_usage_mb, 14848);
    }

    #[test]
    fn validation() {
        let ok = validate_memory(
            &MemorySettings {
                min_mb: 1024,
                max_mb: 4096,
            },
            GB16,
        );
        assert!(ok.is_valid() && ok.warnings.is_empty());

        let low = validate_memory(
            &MemorySettings {
                min_mb: 512,
                max_mb: 4096,
            },
            GB16,
        );
        assert_eq!(
            low.errors,
            [MemoryIssue::MinTooLow {
                min_allowed_mb: 1024
            }]
        );

        let high_min = validate_memory(
            &MemorySettings {
                min_mb: 9000,
                max_mb: 10000,
            },
            GB16,
        );
        assert_eq!(
            high_min.errors,
            [MemoryIssue::MinTooHigh {
                max_allowed_mb: 8192
            }]
        );

        let inverted = validate_memory(
            &MemorySettings {
                min_mb: 4096,
                max_mb: 2048,
            },
            GB16,
        );
        assert_eq!(inverted.errors, [MemoryIssue::MaxBelowMin]);

        let too_high = validate_memory(
            &MemorySettings {
                min_mb: 1024,
                max_mb: 16384,
            },
            GB16,
        );
        assert_eq!(
            too_high.errors,
            [MemoryIssue::MaxTooHigh {
                max_allowed_mb: 15360
            }]
        );

        let warn = validate_memory(
            &MemorySettings {
                min_mb: 1024,
                max_mb: 15000,
            },
            GB16,
        );
        assert!(warn.is_valid());
        assert_eq!(
            warn.warnings,
            [MemoryIssue::HighUsage {
                threshold_mb: 14848
            }]
        );
    }

    #[test]
    fn recommendations_and_clamping() {
        assert_eq!(
            recommended_memory(GB16, false),
            MemorySettings {
                min_mb: 1024,
                max_mb: 4096
            }
        );
        assert_eq!(
            recommended_memory(GB16, true),
            MemorySettings {
                min_mb: 1024,
                max_mb: 6144
            }
        );
        assert_eq!(
            recommended_memory(4 * 1024, true),
            MemorySettings {
                min_mb: 1024,
                max_mb: 2048
            }
        );
        let tiny = recommended_memory(2 * 1024, false);
        assert_eq!(
            tiny,
            MemorySettings {
                min_mb: 1024,
                max_mb: 1024
            }
        );
        for total in [2 * 1024, 4 * 1024, 8 * 1024, GB16, 64 * 1024] {
            for modded in [false, true] {
                let r = recommended_memory(total, modded);
                assert!(
                    validate_memory(&r, total).is_valid(),
                    "{total} {modded} {r:?}"
                );
            }
        }
        let fixed = clamp_memory(
            &MemorySettings {
                min_mb: 100,
                max_mb: 99999,
            },
            GB16,
        );
        assert_eq!(
            fixed,
            MemorySettings {
                min_mb: 1024,
                max_mb: 15360
            }
        );
        assert!(system_memory_mb() > 0);
    }

    #[test]
    fn automatic_memory() {
        let gb = |n: u64| n * 1024 - 120;
        let max = |total: u64, modded: bool, mods: usize| auto_memory(total, modded, mods).max_mb;
        // Vanilla follows the RAM.
        assert_eq!(max(gb(4), false, 0), 2048);
        assert_eq!(max(gb(8), false, 0), 3072);
        assert_eq!(max(gb(16), false, 0), 4096);
        assert_eq!(max(gb(64), false, 0), 4096);
        // Modded follows the mods, within what the machine can spare.
        assert_eq!(max(gb(16), true, 0), 3072);
        assert_eq!(max(gb(16), true, 12), 3072);
        assert_eq!(max(gb(16), true, 40), 4096);
        assert_eq!(max(gb(16), true, 90), 5120);
        assert_eq!(max(gb(16), true, 150), 6144);
        assert_eq!(max(gb(16), true, 220), 8192);
        assert_eq!(max(gb(16), true, 400), 10240);
        assert_eq!(max(gb(64), true, 900), AUTO_MAX_MB);
        // Small machines: half of the RAM at most.
        assert_eq!(max(gb(8), true, 220), 4096);
        assert_eq!(max(gb(6), true, 90), 3072);
        assert_eq!(max(gb(4), true, 90), 2048);
        assert_eq!(max(gb(2), true, 90), 1024);
        assert_eq!(max(gb(12), true, 220), 6144);
        // More mods never means less memory, and the result is always a valid setting.
        for total in [gb(2), gb(4), gb(6), gb(8), gb(12), gb(16), gb(32), gb(64)] {
            let mut previous = 0;
            for mods in [0, 1, 15, 16, 50, 51, 100, 101, 160, 161, 250, 251, 1000] {
                let auto = auto_memory(total, true, mods);
                assert!(auto.max_mb >= previous, "{total} {mods}");
                assert_eq!(auto.max_mb % 512, 0);
                assert!(
                    validate_memory(&auto, total).is_valid(),
                    "{total} {mods} {auto:?}"
                );
                previous = auto.max_mb;
            }
            assert!(validate_memory(&auto_memory(total, false, 0), total).is_valid());
        }
    }

    #[test]
    fn another_game_fits_only_with_room_for_its_heap_and_the_jvm() {
        // An 8 GB computer with one modded game on: about 1.5 GB left.
        assert!(!fits_another_game(1536, 4096));
        assert!(
            !fits_another_game(4096, 4096),
            "the heap alone is not enough"
        );
        assert!(fits_another_game(5120, 4096));
        assert!(fits_another_game(7000, 4096));
        assert!(!fits_another_game(0, 1024));
        assert!(available_memory_mb() <= system_memory_mb());
    }

    #[test]
    fn pack_recommendation_raises_automatic_memory() {
        let gb = |n: u64| n * 1024 - 120;
        let auto = auto_memory(gb(16), true, 40);
        assert_eq!(auto.max_mb, 4096);
        assert_eq!(auto_memory_at_least(auto, 6144, gb(16)).max_mb, 6144);
        // Never past what the machine can spare, never below what "Otomatik" picked.
        assert_eq!(auto_memory_at_least(auto, 12_000, gb(16)).max_mb, 10240);
        assert_eq!(auto_memory_at_least(auto, 2048, gb(16)).max_mb, 4096);
        let small = auto_memory(gb(8), true, 40);
        assert_eq!(auto_memory_at_least(small, 6144, gb(8)).max_mb, 4096);
        assert!(validate_memory(&auto_memory_at_least(auto, 6000, gb(16)), gb(16)).is_valid());
    }
}
