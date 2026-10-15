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
}
