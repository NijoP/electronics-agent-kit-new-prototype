//! Dashboard data model: rolling history, statistics, link health and alarms.
//!
//! Fixed capacity, allocation-free, so it can run on the host dashboard and (if needed) on
//! the device itself.

use crate::frame::Frame;

/// Number of samples kept (at 1 Hz: four minutes of history).
pub const HISTORY: usize = 240;
/// A link is reported stale when no valid frame arrived for this long.
pub const STALE_AFTER_MS: u64 = 3_000;

/// Alarm thresholds. Defaults are comfort-zone values for an indoor monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub temp_high_mc: i32,
    pub temp_low_mc: i32,
    pub rh_high_mpct: u32,
    /// USB 2.0 lower VBUS limit at the device is 4.75 V; below 4.5 V the LDO (AMS1117,
    /// dropout up to ~1.3 V at 800 mA) may fall out of regulation.
    pub vbus_low_mv: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            temp_high_mc: 30_000,
            temp_low_mc: 15_000,
            rh_high_mpct: 70_000,
            vbus_low_mv: 4_500,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alarm {
    TemperatureHigh,
    TemperatureLow,
    HumidityHigh,
    VbusLow,
    SensorFault,
    LinkStale,
    SequenceGap,
}

impl Alarm {
    pub fn as_str(self) -> &'static str {
        match self {
            Alarm::TemperatureHigh => "temperature_high",
            Alarm::TemperatureLow => "temperature_low",
            Alarm::HumidityHigh => "humidity_high",
            Alarm::VbusLow => "vbus_low",
            Alarm::SensorFault => "sensor_fault",
            Alarm::LinkStale => "link_stale",
            Alarm::SequenceGap => "sequence_gap",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub count: u32,
    pub min: i64,
    pub max: i64,
    pub mean: i64,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    host_ms: u64,
    frame: Frame,
}

pub struct Dashboard {
    ring: [Option<Entry>; HISTORY],
    head: usize,
    len: usize,
    pub thresholds: Thresholds,
    pub frames_ok: u64,
    pub frames_rejected: u64,
    pub sequence_gaps: u64,
    last_seq: Option<u32>,
    gap_on_last: bool,
}

impl Default for Dashboard {
    fn default() -> Self {
        Self::new(Thresholds::default())
    }
}

impl Dashboard {
    pub const fn new(thresholds: Thresholds) -> Self {
        Dashboard {
            ring: [None; HISTORY],
            head: 0,
            len: 0,
            thresholds,
            frames_ok: 0,
            frames_rejected: 0,
            sequence_gaps: 0,
            last_seq: None,
            gap_on_last: false,
        }
    }

    /// Record a frame that failed to parse.
    pub fn reject(&mut self) {
        self.frames_rejected += 1;
    }

    /// Ingest a valid frame received at host time `host_ms`.
    pub fn ingest(&mut self, frame: Frame, host_ms: u64) {
        self.frames_ok += 1;
        self.gap_on_last = match self.last_seq {
            // seq restarting at 1 means the device rebooted: not a gap.
            Some(prev) if frame.seq != prev.wrapping_add(1) && frame.seq != 1 => {
                self.sequence_gaps += 1;
                true
            }
            _ => false,
        };
        self.last_seq = Some(frame.seq);
        self.ring[self.head] = Some(Entry { host_ms, frame });
        self.head = (self.head + 1) % HISTORY;
        self.len = (self.len + 1).min(HISTORY);
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Samples oldest first.
    pub fn iter(&self) -> impl Iterator<Item = (u64, &Frame)> + '_ {
        let start = (self.head + HISTORY - self.len) % HISTORY;
        (0..self.len).filter_map(move |i| {
            self.ring[(start + i) % HISTORY]
                .as_ref()
                .map(|e| (e.host_ms, &e.frame))
        })
    }

    pub fn latest(&self) -> Option<(u64, &Frame)> {
        if self.len == 0 {
            return None;
        }
        self.ring[(self.head + HISTORY - 1) % HISTORY]
            .as_ref()
            .map(|e| (e.host_ms, &e.frame))
    }

    fn stats_of(&self, f: impl Fn(&Frame) -> Option<i64>) -> Option<Stats> {
        let mut s = Stats {
            count: 0,
            min: i64::MAX,
            max: i64::MIN,
            mean: 0,
        };
        let mut sum: i64 = 0;
        for (_, fr) in self.iter() {
            if let Some(v) = f(fr) {
                s.count += 1;
                s.min = s.min.min(v);
                s.max = s.max.max(v);
                sum += v;
            }
        }
        if s.count == 0 {
            return None;
        }
        s.mean = sum / s.count as i64;
        Some(s)
    }

    pub fn temperature_stats(&self) -> Option<Stats> {
        self.stats_of(|f| f.temperature_mc.map(i64::from))
    }
    pub fn humidity_stats(&self) -> Option<Stats> {
        self.stats_of(|f| f.humidity_mpct.map(i64::from))
    }
    pub fn vbus_stats(&self) -> Option<Stats> {
        self.stats_of(|f| f.vbus_mv.map(i64::from))
    }

    /// Active alarms at host time `now_ms`, written into `out`; returns how many.
    pub fn alarms(&self, now_ms: u64, out: &mut [Alarm; 7]) -> usize {
        let mut n = 0;
        let mut push = |a| {
            out[n] = a;
            n += 1;
        };
        match self.latest() {
            None => push(Alarm::LinkStale),
            Some((t, f)) => {
                if now_ms.saturating_sub(t) > STALE_AFTER_MS {
                    push(Alarm::LinkStale);
                }
                match f.temperature_mc {
                    Some(v) if v > self.thresholds.temp_high_mc => push(Alarm::TemperatureHigh),
                    Some(v) if v < self.thresholds.temp_low_mc => push(Alarm::TemperatureLow),
                    Some(_) => {}
                    None => push(Alarm::SensorFault),
                }
                if matches!(f.humidity_mpct, Some(h) if h > self.thresholds.rh_high_mpct) {
                    push(Alarm::HumidityHigh);
                }
                if matches!(f.vbus_mv, Some(v) if v < self.thresholds.vbus_low_mv) {
                    push(Alarm::VbusLow);
                }
                if self.gap_on_last {
                    push(Alarm::SequenceGap);
                }
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fr(seq: u32, t: Option<i32>) -> Frame {
        Frame {
            seq,
            uptime_ms: seq * 1000,
            temperature_mc: t,
            humidity_mpct: t.map(|_| 40_000),
            vbus_mv: Some(5_000),
            button_pressed: false,
            sensor_faults: 0,
        }
    }

    #[test]
    fn statistics_ignore_missing_samples() {
        let mut d = Dashboard::default();
        d.ingest(fr(1, Some(20_000)), 1000);
        d.ingest(fr(2, None), 2000);
        d.ingest(fr(3, Some(24_000)), 3000);
        let s = d.temperature_stats().unwrap();
        assert_eq!((s.count, s.min, s.max, s.mean), (2, 20_000, 24_000, 22_000));
    }

    #[test]
    fn ring_buffer_keeps_newest_history() {
        let mut d = Dashboard::default();
        for i in 1..=(HISTORY as u32 + 10) {
            d.ingest(fr(i, Some(i as i32)), i as u64);
        }
        assert_eq!(d.len(), HISTORY);
        assert_eq!(d.iter().next().unwrap().1.seq, 11);
        assert_eq!(d.latest().unwrap().1.seq, HISTORY as u32 + 10);
    }

    #[test]
    fn alarms_cover_thresholds_faults_staleness_and_gaps() {
        let mut d = Dashboard::default();
        let mut a = [Alarm::LinkStale; 7];
        assert_eq!(d.alarms(0, &mut a), 1);
        assert_eq!(a[0], Alarm::LinkStale);
        d.ingest(fr(1, Some(35_000)), 1000);
        let n = d.alarms(1500, &mut a);
        assert_eq!(&a[..n], &[Alarm::TemperatureHigh]);
        d.ingest(fr(3, None), 2000);
        let n = d.alarms(2100, &mut a);
        assert_eq!(&a[..n], &[Alarm::SensorFault, Alarm::SequenceGap]);
        let n = d.alarms(9000, &mut a);
        assert!(a[..n].contains(&Alarm::LinkStale));
        assert_eq!(d.sequence_gaps, 1);
        // Device reboot (seq back to 1) is not a gap.
        d.ingest(fr(1, Some(20_000)), 9500);
        assert_eq!(d.sequence_gaps, 1);
    }
}
