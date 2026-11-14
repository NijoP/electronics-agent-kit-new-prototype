//! Central resource profile and workload classes (performance directive §4–5,
//! `docs/development/PERFORMANCE.md` §3).
//!
//! One place decides how much background work EAK runs at once, the cache budget, the 3D
//! render tier and the terminal scrollback the UI keeps. Subsystems read the resolved
//! [`ResourceProfile`]; none sizes itself. The profile only changes presentation and
//! scheduling, never an engineering value, a geometry or a validation outcome.
//!
//! Detection is deliberately minimal: the logical CPU count from
//! `std::thread::available_parallelism`. Memory and GPU limits are not queried (no portable,
//! reliable source without a large dependency); the user can pick a mode instead. AUTO is
//! resolved once at startup and when the preference changes, so it never oscillates.
//!
//! Workload classes ([`Priority`]) map onto the work EAK already schedules:
//! * P0 interactive reads (route preview, library search): their own blocking thread, no queue.
//! * P1 project mutations and persistence: their own blocking thread, no queue, never dropped;
//!   the UI awaits each one, so order is preserved.
//! * P2 required validation and analysis (ERC/DRC/DFM, connectivity, domain): bounded by
//!   `analysis_jobs`.
//! * P3–P5 background (manufacturing export, simulation, sourcing): bounded by `background_jobs`.
//!   A queued job waits; it is never skipped, so a required check still runs.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

/// User preference. These are preferences, not promises of a frame rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceMode {
    #[default]
    Auto,
    LowResource,
    Balanced,
    HighPerformance,
    Custom,
}

/// Limits the user sets in CUSTOM mode (clamped to safe ranges on use).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CustomLimits {
    pub analysis_jobs: usize,
    pub background_jobs: usize,
    pub cache_budget_mb: u32,
    pub pixel_ratio_cap: f32,
    pub antialias: bool,
    pub scrollback_lines: u32,
}

impl Default for CustomLimits {
    fn default() -> Self {
        Self {
            analysis_jobs: 2,
            background_jobs: 1,
            cache_budget_mb: 256,
            pixel_ratio_cap: 1.5,
            antialias: true,
            scrollback_lines: 5000,
        }
    }
}

/// Persisted preference (`performance.json` in the app config directory).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct PerformanceSettings {
    pub mode: PerformanceMode,
    #[serde(default)]
    pub custom: CustomLimits,
}

/// Effective tier after AUTO is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    LowResource,
    Balanced,
    HighPerformance,
    Custom,
}

/// Workload priority classes (directive §5). Lower number = more urgent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Priority {
    /// User interaction, input processing, active editor updates.
    P0,
    /// Project mutations, persistence, critical UI operations.
    P1,
    /// Required engineering validation and interactive calculations.
    P2,
    /// Background indexing, nonurgent calculations, 3D asset preparation.
    P3,
    /// Supplier refresh, document ingestion, optional prefetching.
    P4,
    /// Nonessential maintenance and cache cleanup.
    P5,
}

/// The resolved profile every subsystem reads.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResourceProfile {
    pub mode: PerformanceMode,
    pub tier: Tier,
    /// Logical CPUs reported by the OS (1 if unknown).
    pub logical_cpus: usize,
    /// Concurrent P2 jobs (validation, analysis).
    pub analysis_jobs: usize,
    /// Concurrent P3–P5 jobs (export, simulation, sourcing).
    pub background_jobs: usize,
    /// Budget for optional caches, MiB (3D model and derived-data caches size to this).
    pub cache_budget_mb: u32,
    /// Cap on the 3D back-buffer device-pixel ratio (canvas CSS size is unchanged).
    pub pixel_ratio_cap: f32,
    pub antialias: bool,
    /// Lines each terminal view keeps (the backend ring per session is a separate, fixed 2 MiB).
    pub scrollback_lines: u32,
    /// What detection could not determine (shown to the user, never guessed).
    pub limitations: Vec<String>,
}

/// Logical CPUs, or 1 when the OS does not say.
pub fn logical_cpus() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

struct Limits {
    analysis_jobs: usize,
    background_jobs: usize,
    cache_budget_mb: u32,
    pixel_ratio_cap: f32,
    antialias: bool,
    scrollback_lines: u32,
}

/// Resolve a preference against the machine. Pure, so it is unit-tested.
pub fn resolve(settings: &PerformanceSettings, cpus: usize) -> ResourceProfile {
    let cpus = cpus.max(1);
    let tier = match settings.mode {
        PerformanceMode::Auto if cpus <= 2 => Tier::LowResource,
        PerformanceMode::Auto if cpus <= 8 => Tier::Balanced,
        PerformanceMode::Auto => Tier::HighPerformance,
        PerformanceMode::LowResource => Tier::LowResource,
        PerformanceMode::Balanced => Tier::Balanced,
        PerformanceMode::HighPerformance => Tier::HighPerformance,
        PerformanceMode::Custom => Tier::Custom,
    };
    // Leave a core for the UI thread and the WebView wherever there is more than one.
    let spare = cpus.saturating_sub(1).max(1);
    let l = match tier {
        Tier::LowResource => Limits {
            analysis_jobs: 1,
            background_jobs: 1,
            cache_budget_mb: 64,
            pixel_ratio_cap: 1.0,
            antialias: false,
            scrollback_lines: 2_000,
        },
        Tier::Balanced => Limits {
            analysis_jobs: (cpus / 2).clamp(1, 3),
            background_jobs: 1,
            cache_budget_mb: 256,
            pixel_ratio_cap: 1.5,
            antialias: true,
            scrollback_lines: 5_000,
        },
        Tier::HighPerformance => Limits {
            analysis_jobs: spare.clamp(2, 6),
            background_jobs: (cpus / 4).clamp(1, 3),
            cache_budget_mb: 1024,
            pixel_ratio_cap: 2.0,
            antialias: true,
            scrollback_lines: 10_000,
        },
        Tier::Custom => {
            let c = settings.custom;
            Limits {
                analysis_jobs: c.analysis_jobs.clamp(1, spare),
                background_jobs: c.background_jobs.clamp(1, spare),
                cache_budget_mb: c.cache_budget_mb.clamp(32, 8192),
                pixel_ratio_cap: if c.pixel_ratio_cap.is_finite() {
                    c.pixel_ratio_cap.clamp(0.5, 3.0)
                } else {
                    1.0
                },
                antialias: c.antialias,
                scrollback_lines: c.scrollback_lines.clamp(500, 100_000),
            }
        }
    };
    ResourceProfile {
        mode: settings.mode,
        tier,
        logical_cpus: cpus,
        analysis_jobs: l.analysis_jobs.min(spare),
        background_jobs: l.background_jobs.min(spare),
        cache_budget_mb: l.cache_budget_mb,
        pixel_ratio_cap: l.pixel_ratio_cap,
        antialias: l.antialias,
        scrollback_lines: l.scrollback_lines,
        limitations: vec![
            "system memory is not measured; choose Low resource on machines with 8 GB or less"
                .into(),
            "GPU memory cannot be queried portably from WebGL; 3D quality follows the mode".into(),
        ],
    }
}

struct Live {
    settings: PerformanceSettings,
    profile: ResourceProfile,
    analysis: Arc<Semaphore>,
    background: Arc<Semaphore>,
}

impl Live {
    fn new(settings: PerformanceSettings) -> Live {
        let profile = resolve(&settings, logical_cpus());
        Live {
            analysis: Arc::new(Semaphore::new(profile.analysis_jobs)),
            background: Arc::new(Semaphore::new(profile.background_jobs)),
            settings,
            profile,
        }
    }
}

/// Managed Tauri state: the preference, its resolved profile and the job limits.
pub struct Resources {
    file: Option<PathBuf>,
    live: Mutex<Live>,
}

impl Resources {
    /// Load the saved preference (a missing or unreadable file means AUTO).
    pub fn load(file: Option<PathBuf>) -> Resources {
        let settings = file
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|s| serde_json::from_str::<PerformanceSettings>(&s).ok())
            .unwrap_or_default();
        Resources {
            file,
            live: Mutex::new(Live::new(settings)),
        }
    }

    fn live(&self) -> std::sync::MutexGuard<'_, Live> {
        // Plain data replaced whole; a panic elsewhere cannot leave it half-written.
        self.live.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn settings(&self) -> PerformanceSettings {
        self.live().settings
    }

    pub fn profile(&self) -> ResourceProfile {
        self.live().profile.clone()
    }

    /// Change the preference: persist it (temp file + rename), re-resolve, resize the limits.
    /// Jobs already running keep their permit on the old limit and finish normally.
    pub fn set(&self, settings: PerformanceSettings) -> Result<ResourceProfile, String> {
        if let Some(f) = &self.file {
            let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
            if let Some(dir) = f.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let tmp = f.with_extension("json.tmp");
            std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, f).map_err(|e| e.to_string())?;
        }
        let mut live = self.live();
        *live = Live::new(settings);
        Ok(live.profile.clone())
    }

    /// The limit a job of this class waits on; `None` for P0/P1, which never queue.
    pub fn limit(&self, priority: Priority) -> Option<Arc<Semaphore>> {
        let live = self.live();
        match priority {
            Priority::P0 | Priority::P1 => None,
            Priority::P2 => Some(Arc::clone(&live.analysis)),
            Priority::P3 | Priority::P4 | Priority::P5 => Some(Arc::clone(&live.background)),
        }
    }
}

/// Tauri wrappers for the settings control.
pub mod cmd {
    use super::*;
    use tauri::State;

    #[tauri::command(rename_all = "snake_case")]
    pub fn resource_profile(state: State<'_, Resources>) -> ResourceProfile {
        state.profile()
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn performance_settings(state: State<'_, Resources>) -> PerformanceSettings {
        state.settings()
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn set_performance_settings(
        state: State<'_, Resources>,
        settings: PerformanceSettings,
    ) -> Result<ResourceProfile, String> {
        state.set(settings)
    }
}

#[cfg(test)]
mod tests {
    // Written, not run (no-build directive).
    use super::*;

    fn mode(m: PerformanceMode) -> PerformanceSettings {
        PerformanceSettings {
            mode: m,
            custom: CustomLimits::default(),
        }
    }

    #[test]
    fn auto_follows_cpu_count_and_keeps_a_core_free() {
        assert_eq!(
            resolve(&mode(PerformanceMode::Auto), 2).tier,
            Tier::LowResource
        );
        assert_eq!(
            resolve(&mode(PerformanceMode::Auto), 4).tier,
            Tier::Balanced
        );
        assert_eq!(
            resolve(&mode(PerformanceMode::Auto), 16).tier,
            Tier::HighPerformance
        );
        for m in [
            PerformanceMode::Auto,
            PerformanceMode::LowResource,
            PerformanceMode::Balanced,
            PerformanceMode::HighPerformance,
            PerformanceMode::Custom,
        ] {
            for cpus in 1..=64 {
                let p = resolve(&mode(m), cpus);
                assert!(p.analysis_jobs >= 1 && p.background_jobs >= 1);
                assert!(cpus == 1 || p.analysis_jobs < cpus, "{m:?} {cpus}");
            }
        }
    }

    #[test]
    fn custom_values_are_clamped() {
        let s = PerformanceSettings {
            mode: PerformanceMode::Custom,
            custom: CustomLimits {
                analysis_jobs: 0,
                background_jobs: 999,
                cache_budget_mb: 0,
                pixel_ratio_cap: f32::NAN,
                antialias: false,
                scrollback_lines: 1,
            },
        };
        let p = resolve(&s, 4);
        assert_eq!(p.analysis_jobs, 1);
        assert_eq!(p.background_jobs, 3);
        assert_eq!(p.cache_budget_mb, 32);
        assert_eq!(p.pixel_ratio_cap, 1.0);
        assert_eq!(p.scrollback_lines, 500);
    }

    #[test]
    fn preference_round_trips_through_the_file() {
        let f = std::env::temp_dir().join(format!("eak-perf-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&f);
        assert_eq!(
            Resources::load(Some(f.clone())).settings().mode,
            PerformanceMode::Auto
        );
        Resources::load(Some(f.clone()))
            .set(mode(PerformanceMode::LowResource))
            .unwrap();
        let r = Resources::load(Some(f.clone()));
        assert_eq!(r.profile().tier, Tier::LowResource);
        assert!(r.limit(Priority::P1).is_none());
        assert_eq!(r.limit(Priority::P2).unwrap().available_permits(), 1);
        let _ = std::fs::remove_file(&f);
    }
}
