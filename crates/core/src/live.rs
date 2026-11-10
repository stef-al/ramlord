//! The running tower: one snapshot after another in, a frame out. The
//! Windows layer feeds it every 2 seconds; `--self-test` drives it off
//! screen. It keeps hues and the smoother between snapshots.

use crate::building::{Building, BuildingError};
use crate::layout::{HueBook, Layout};
use crate::palette::SKY;
use crate::render::{Frame, rasterize};
use crate::rules::Rules;
use crate::scene;
use crate::smooth::Smoother;
use crate::snapshot::Snapshot;
use crate::tenants::{Lease, building};

pub struct Live {
    rules: Rules,
    lease: Lease,
    smoother: Smoother,
    hues: HueBook,
    current: Option<Building>,
    frame: Option<Frame>,
}

impl Live {
    pub fn new(rules: Rules, lease: Lease) -> Live {
        Live {
            rules,
            lease,
            smoother: Smoother::new(),
            hues: HueBook::new(),
            current: None,
            frame: None,
        }
    }

    /// Groups `snapshot`, lays out the smoothed building and draws the tower
    /// with current numbers. Returns whether the frame changed.
    pub fn update(&mut self, snapshot: &Snapshot) -> Result<bool, BuildingError> {
        let current = building(snapshot, &self.rules, &self.lease);
        current.validate()?;
        let smoothed = self.smoother.smooth(snapshot.at_ms, &current);
        let order = self.smoother.order(snapshot.at_ms, &smoothed);
        let (layout, hues) = Layout::ordered(&smoothed, &self.hues, &order);
        self.hues = hues;
        // The smoothed building lists the same tenants in the same order,
        // so the layout's tenant indices hold for `current` too.
        let draws = scene::tower(&current, &layout);
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

    /// The last building, with current numbers.
    pub fn building(&self) -> Option<&Building> {
        self.current.as_ref()
    }

    pub fn hues(&self) -> &HueBook {
        &self.hues
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
        let first = live.hues().clone();
        live.update(&snapshot(2_000, 100)).unwrap(); // Slack now leads
        assert_eq!(live.hues(), &first);
        assert_eq!(live.building().unwrap().tenants.len(), 2);
    }
}
