//! The running tower: one snapshot after another in, a frame out. The
//! Windows layer feeds it a snapshot every 2 seconds and the foreground
//! window every second; `--self-test` drives it off screen. Between
//! snapshots it keeps watched time, flags and hues (the state file), the
//! remark, and the smoother.

use std::collections::BTreeSet;

use crate::building::{Building, BuildingError};
use crate::flags::{State, Watch};
use crate::layout::Layout;
use crate::palette::SKY;
use crate::remarks::{Kind, Remarks, Sample};
use crate::render::{Frame, rasterize};
use crate::rules::Rules;
use crate::scene;
use crate::smooth::Smoother;
use crate::snapshot::Snapshot;
use crate::tenants::{Lease, building_of, group};

pub struct Live {
    rules: Rules,
    lease: Lease,
    watch: Watch,
    remarks: Remarks,
    smoother: Smoother,
    current: Option<Building>,
    frame: Option<Frame>,
}

impl Live {
    /// `state` is what the last run saved (or `State::default()`);
    /// `family` lists tenant ids the user marked as Family.
    pub fn new(rules: Rules, lease: Lease, state: State, family: BTreeSet<String>) -> Live {
        Live {
            rules,
            lease,
            watch: Watch::new(state, family),
            remarks: Remarks::new(),
            smoother: Smoother::new(),
            current: None,
            frame: None,
        }
    }

    /// Credits focus to the tenant owning `pid`; call once a second.
    pub fn focus(&mut self, pid: u32) {
        self.watch.focus(pid);
    }

    /// Groups `snapshot`, advances watched time and flags, picks the
    /// remark, lays out the smoothed building and draws the tower with
    /// current numbers. Returns whether the frame changed.
    pub fn update(&mut self, snapshot: &Snapshot) -> Result<bool, BuildingError> {
        let grouping = group(snapshot, &self.rules);
        let gained = self.watch.observe(snapshot, &grouping);
        let mut current = building_of(snapshot, grouping, &self.lease);
        for tenant in &mut current.tenants {
            tenant.flags = self.watch.flags_of(&tenant.id).to_vec();
        }
        let sample = Sample {
            watched_ms: self.watch.state().watched_ms,
            commit_used_bytes: snapshot.commit_used_bytes,
            commit_limit_bytes: snapshot.commit_limit_bytes,
            signed_in_at_ms: snapshot.signed_in_at_ms,
            gained: &gained,
        };
        let mut told = self.watch.state().squatters_told;
        let remark = self.remarks.update(&current, &sample, &mut told).clone();
        self.watch.state_mut().squatters_told = told;
        current.remark = remark.text;
        current.validate()?;

        let smoothed = self.smoother.smooth(snapshot.at_ms, &current);
        let order = self.smoother.order(snapshot.at_ms, &smoothed);
        let (layout, hues) = Layout::ordered(&smoothed, &self.watch.state().hues(), &order);
        self.watch.state_mut().keep_hues(&hues);
        let highlight = match (remark.kind, remark.tenant) {
            (Kind::Full, Some(id)) => current.tenants.iter().position(|t| t.id == id),
            _ => None,
        };
        // The smoothed building lists the same tenants in the same order,
        // so the layout's tenant indices hold for `current` too.
        let draws = scene::tower_highlighting(&current, &layout, highlight);
        let frame = rasterize(scene::WIDTH, scene::HEIGHT, SKY, &draws);
        let changed = self.frame.as_ref() != Some(&frame);
        self.frame = Some(frame);
        self.current = Some(current);
        Ok(changed)
    }

    /// The last frame drawn.
    pub fn frame(&self) -> Option<&Frame> {
        self.frame.as_ref()
    }

    /// The last building, with current numbers, flags and the remark.
    pub fn building(&self) -> Option<&Building> {
        self.current.as_ref()
    }

    /// What to save in `state.json`.
    pub fn state(&self) -> &State {
        self.watch.state()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::Price;
    use crate::snapshot::Process;

    const MIB: u64 = 1 << 20;

    fn snapshot(at_ms: u64, chrome_mib: u64) -> Snapshot {
        let process = |pid, parent, path: &str, mib| Process {
            pid,
            parent_pid: parent,
            started_at_ms: u64::from(pid),
            session: 1,
            image_path: Some(path.into()),
            description: None,
            rent_bytes: mib * MIB,
            lease_bytes: mib * MIB,
            has_visible_window: true,
            is_self: false,
            ..Default::default()
        };
        Snapshot {
            at_ms,
            capacity_bytes: 8192 * MIB,
            available_bytes: 4096 * MIB,
            processes: vec![
                process(10, 1, r"C:\Windows\explorer.exe", 100),
                process(20, 10, r"C:\Chrome\chrome.exe", chrome_mib),
                process(30, 10, r"C:\Slack\slack.exe", 400),
            ],
            ..Snapshot::default()
        }
    }

    fn live() -> Live {
        Live::new(
            Rules::builtin(),
            Lease {
                memory_type: "RAM".into(),
                price: Price::placeholder(),
                remark: String::new(),
            },
            State::default(),
            BTreeSet::new(),
        )
    }

    #[test]
    fn the_frame_changes_only_when_the_scene_does() {
        let mut live = live();
        assert_eq!(live.update(&snapshot(0, 1000)), Ok(true));
        assert_eq!(live.update(&snapshot(2_000, 1000)), Ok(false));
        assert_eq!(live.update(&snapshot(4_000, 3000)), Ok(true));
        let frame = live.frame().unwrap();
        assert_eq!((frame.width, frame.height), (384, 240));
    }

    #[test]
    fn hues_are_kept_between_snapshots() {
        let mut live = live();
        live.update(&snapshot(0, 1000)).unwrap();
        let first = live.state().hues();
        live.update(&snapshot(2_000, 100)).unwrap(); // Slack now leads
        assert_eq!(live.state().hues(), first);
        assert_eq!(live.building().unwrap().tenants.len(), 2);
    }
}
