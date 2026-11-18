use std::{
    path::PathBuf,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::git::RefSnapshot;

pub struct RefMonitor {
    path: PathBuf,
    interval: Duration,
    next_probe: Instant,
    displayed: Option<RefSnapshot>,
    pending: Option<RefSnapshot>,
    in_flight: Option<JoinHandle<Option<RefSnapshot>>>,
    discard_in_flight: bool,
}

impl RefMonitor {
    pub fn new(path: PathBuf, interval: Duration) -> Self {
        // Sample before loading history, so a change during the load is not missed.
        let displayed = RefSnapshot::read(&path);
        Self {
            path,
            interval,
            next_probe: Instant::now() + interval,
            displayed,
            pending: None,
            in_flight: None,
            discard_in_flight: false,
        }
    }

    pub fn reset(&mut self, path: PathBuf, displayed: Option<RefSnapshot>) {
        self.path = path;
        self.displayed = displayed;
        self.pending = None;
        self.next_probe = Instant::now() + self.interval;
        // Do not wait for a previous probe, or spawn a second one over it.
        self.discard_in_flight = self.in_flight.is_some();
    }

    pub fn rebaseline(&mut self) {
        let displayed = RefSnapshot::read(&self.path);
        self.reset(self.path.clone(), displayed);
    }

    pub fn tick(&mut self, now: Instant) {
        if self.in_flight.as_ref().is_some_and(JoinHandle::is_finished) {
            let probe = self.in_flight.take().unwrap();
            if let (Ok(Some(snapshot)), false) = (probe.join(), self.discard_in_flight) {
                self.observe(snapshot);
            }
            self.discard_in_flight = false;
        }
        if self.in_flight.is_none() && now >= self.next_probe {
            self.next_probe = now + self.interval;
            let path = self.path.clone();
            self.in_flight = thread::Builder::new()
                .spawn(move || RefSnapshot::read(&path))
                .ok();
        }
    }

    fn observe(&mut self, snapshot: RefSnapshot) {
        self.pending = (self.displayed.as_ref() != Some(&snapshot)).then_some(snapshot);
    }

    pub fn has_change(&self) -> bool {
        self.pending.is_some()
    }

    pub fn take_change(&mut self) -> Option<RefSnapshot> {
        self.pending.take()
    }

    pub fn refreshed(&mut self, snapshot: RefSnapshot) {
        self.displayed = Some(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::git::GitRepository;

    const INTERVAL: Duration = Duration::from_secs(10);

    fn finish_probe(monitor: &mut RefMonitor, time: Instant) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while monitor.in_flight.is_some() {
            assert!(Instant::now() < deadline, "Git ref probe did not finish");
            monitor.tick(time);
            thread::yield_now();
        }
    }

    #[test]
    fn detects_changes_at_poll_interval_and_retries_unapplied_changes() {
        let dir = tempfile::tempdir().unwrap();
        let git = GitRepository::new(dir.path());
        git.init();
        git.commit("initial", "2026-01-01");

        let mut monitor = RefMonitor::new(dir.path().into(), INTERVAL);
        let first = monitor.next_probe;
        git.commit("new commit", "2026-01-02");
        monitor.tick(first - Duration::from_millis(1));
        assert!(!monitor.has_change());
        assert!(monitor.in_flight.is_none());

        monitor.tick(first);
        finish_probe(&mut monitor, first);
        assert!(monitor.take_change().is_some());
        assert!(!monitor.has_change());

        // The same change is reported again if the reload was not acknowledged.
        let second = first + INTERVAL;
        monitor.tick(second);
        finish_probe(&mut monitor, second);
        let snapshot = monitor.take_change().unwrap();
        monitor.refreshed(snapshot);

        let third = second + INTERVAL;
        monitor.tick(third);
        finish_probe(&mut monitor, third);
        assert!(monitor.take_change().is_none());
    }

    #[test]
    fn ignores_in_flight_probe_when_switching_worktrees() {
        let dir = tempfile::tempdir().unwrap();
        let git = GitRepository::new(dir.path());
        git.init();
        git.commit("initial", "2026-01-01");

        let mut monitor = RefMonitor::new(dir.path().into(), INTERVAL);
        git.commit("new commit", "2026-01-02");
        monitor.tick(monitor.next_probe);
        let new_dir = tempfile::tempdir().unwrap();
        monitor.reset(new_dir.path().into(), None);
        let before_next_probe = monitor.next_probe - Duration::from_millis(1);
        finish_probe(&mut monitor, before_next_probe);
        assert!(!monitor.has_change());
    }

    #[test]
    fn discards_a_deferred_change_if_refs_return_to_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let git = GitRepository::new(dir.path());
        git.init();
        git.commit("initial", "2026-01-01");
        let mut monitor = RefMonitor::new(dir.path().into(), INTERVAL);

        git.checkout_b("temporary");
        monitor.observe(RefSnapshot::read(dir.path()).unwrap());
        assert!(monitor.has_change());
        git.checkout("master");
        git.branch_d("temporary");
        monitor.observe(RefSnapshot::read(dir.path()).unwrap());
        assert!(!monitor.has_change());
    }
}
