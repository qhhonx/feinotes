// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
use std::time::{Duration, Instant};
use super::SyncPhase;

// Allow a full 45-second CloudKit request and its retry. A long gap between
// observations (including app suspension) is not evidence of repeated failures.
const STALL_AFTER: Duration = Duration::from_secs(90);
const MAX_OBSERVATION_GAP: Duration = Duration::from_secs(80);

#[derive(Default)]
struct Watch {
    first: Option<Instant>,
    last: Option<Instant>,
    stalled: Option<SyncPhase>,
}

impl Watch {
    fn observe(&mut self, now: Instant, pending: Option<SyncPhase>, advanced: bool) {
        let Some(phase) = pending else { *self = Self::default(); return };
        if advanced || self.last.is_some_and(|last| now.duration_since(last) > MAX_OBSERVATION_GAP) {
            *self = Self::default();
        }
        let first = *self.first.get_or_insert(now);
        self.last = Some(now);
        self.stalled = (now.duration_since(first) >= STALL_AFTER).then_some(phase);
    }
}

#[derive(Default)]
pub(super) struct Monitor {
    transport: Watch,
    legacy_transport: Watch,
    apply: Watch,
    upload: Watch,
    needs_success: bool,
    pub last_success: Option<u64>,
}

impl Monitor {
    pub fn new(last_success: Option<u64>) -> Self { Self { last_success, ..Self::default() } }

    pub fn stalled(&self) -> Option<SyncPhase> { self.legacy_transport.stalled.or(self.transport.stalled).or(self.apply.stalled).or(self.upload.stalled) }

    fn transport(&mut self, legacy: bool) -> &mut Watch {
        if legacy { &mut self.legacy_transport } else { &mut self.transport }
    }

    pub fn failed(&mut self, now: Instant, phase: SyncPhase, round: Round) {
        self.needs_success = true;
        if round.uploaded { self.upload = Watch::default(); }
        if phase == SyncPhase::Applying {
            if round.connected { *self.transport(round.legacy) = Watch::default(); }
            self.apply.observe(now, Some(phase), round.applied);
        } else {
            if round.applied { self.apply = Watch::default(); }
            self.transport(round.legacy).observe(now, Some(phase), round.download_advanced);
        }
    }

    pub fn completed(&mut self, now: Instant, round: Round) {
        self.transport(round.legacy).observe(now, round.downloading.then_some(SyncPhase::Downloading), round.download_advanced);
        // While catching up, the apply queue may be waiting for dependencies.
        if round.apply_blocked_by_local_edits || round.downloading && round.download_advanced {
            self.apply = Watch::default();
        } else {
            self.apply.observe(now, round.applying.then_some(SyncPhase::Applying), round.applied);
        }
        if !round.legacy {
            self.upload.observe(now, round.uploading.then_some(SyncPhase::Uploading), round.uploaded);
        }
        self.needs_success |= round.download_advanced || round.applied || round.uploaded
            || round.downloading || round.applying || round.uploading;
    }

    pub fn should_record_success(&self) -> bool { self.last_success.is_none() || self.needs_success }

    pub fn record_success(&mut self, time: u64) {
        self.last_success = Some(time);
        self.needs_success = false;
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Round {
    pub legacy: bool,
    pub connected: bool,
    pub downloading: bool,
    pub download_advanced: bool,
    pub applying: bool,
    pub applied: bool,
    pub apply_blocked_by_local_edits: bool,
    pub uploading: bool,
    pub uploaded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use SyncPhase::*;

    #[test]
    fn brief_failures_and_suspension_do_not_warn() {
        let start = Instant::now();
        let mut monitor = Monitor::new(Some(1));
        monitor.failed(start, Offline, Round::default());
        monitor.failed(start + Duration::from_secs(30), Offline, Round::default());
        assert_eq!(monitor.stalled(), None);
        monitor.completed(start + Duration::from_secs(36), Round::default());
        assert_eq!(monitor.stalled(), None);
        monitor.failed(start + Duration::from_secs(60), Account, Round::default());
        monitor.failed(start + Duration::from_secs(200), Account, Round::default());
        assert_eq!(monitor.stalled(), None);
    }

    #[test]
    fn persistent_failure_is_scoped_and_clears_when_the_step_recovers() {
        let start = Instant::now();
        let mut personal = Monitor::new(Some(1));
        let mut shared = Monitor::new(Some(2));
        for seconds in [0, 20, 50, 90] {
            let now = start + Duration::from_secs(seconds);
            personal.failed(now, Applying, Round::default());
            shared.completed(now, Round { uploading: true, uploaded: true, ..Round::default() });
        }
        assert_eq!(personal.stalled(), Some(Applying));
        assert_eq!(shared.stalled(), None);
        assert_eq!(personal.last_success, Some(1));
        personal.completed(start + Duration::from_secs(93), Round { applying: true, applied: true, ..Round::default() });
        assert_eq!(personal.stalled(), None);
        assert!(personal.should_record_success());
        personal.record_success(3);
        assert!(!personal.should_record_success());
        assert_eq!(shared.last_success, Some(2));
    }

    #[test]
    fn live_traffic_with_fixed_queue_sizes_keeps_moving() {
        let start = Instant::now();
        let mut monitor = Monitor::default();
        for seconds in [0, 35, 70, 105, 140] {
            monitor.completed(start + Duration::from_secs(seconds), Round {
                applying: true, applied: seconds != 0, uploading: true, uploaded: seconds != 0, ..Round::default()
            });
        }
        assert_eq!(monitor.stalled(), None);
        for seconds in [170, 200, 230] {
            monitor.completed(start + Duration::from_secs(seconds), Round { uploading: true, ..Round::default() });
        }
        assert_eq!(monitor.stalled(), Some(Uploading));
        monitor.completed(start + Duration::from_secs(231), Round { uploading: true, uploaded: true, ..Round::default() });
        assert_eq!(monitor.stalled(), None);
    }

    #[test]
    fn repeated_pages_without_advancing_do_not_count_as_download_progress() {
        let start = Instant::now();
        let mut monitor = Monitor::default();
        for seconds in [0, 35, 70, 105] {
            let now = start + Duration::from_secs(seconds);
            monitor.completed(now, Round { downloading: true, ..Round::default() });
            monitor.completed(now, Round { legacy: true, ..Round::default() });
        }
        assert_eq!(monitor.stalled(), Some(Downloading));
        monitor.completed(start + Duration::from_secs(106), Round { downloading: true, download_advanced: true, ..Round::default() });
        assert_eq!(monitor.stalled(), None);
    }
}
