//! The running tower: one snapshot after another in, a frame out. The
//! Windows layer feeds it a snapshot every 2 seconds and the foreground
//! window every second, passes on clicks and keys, and runs the commands
//! they produce; `--self-test` drives it off screen. Between snapshots it
//! keeps watched time, flags and hues (the state file), the remark, the
//! smoother, and the selection, details card and eviction (`ui`).

use std::collections::{BTreeMap, BTreeSet};

use crate::building::{Building, BuildingError};
use crate::flags::{BUCKETS, State, Watch};
use crate::layout::Layout;
use crate::palette::SKY;
use crate::remarks::{Kind, Remarks, Sample};
use crate::render::{Frame, rasterize};
use crate::rules::Rules;
use crate::scene::{self, Details, Panel, Roll};
use crate::smooth::Smoother;
use crate::snapshot::Snapshot;
use crate::tenants::{Lease, building_of, group};
use crate::ui::{Command, Context, Eviction, Input, Region, Report, Ui, View};

/// The last sample, laid out, kept so input can be answered between samples.
struct Scene {
    building: Building,
    layout: Layout,
    highlight: Option<usize>,
    at_ms: u64,
    /// Each tenant's processes, by tenant id.
    pids: BTreeMap<String, Vec<u32>>,
}

pub struct Live {
    rules: Rules,
    lease: Lease,
    watch: Watch,
    remarks: Remarks,
    smoother: Smoother,
    ui: Ui,
    scene: Option<Scene>,
    regions: Vec<Region>,
    frame: Option<Frame>,
}

impl Live {
    /// `state` is what the last run saved (or `State::default()`);
    /// `family` lists tenant ids the user marked as Family.
    pub fn new(rules: Rules, lease: Lease, state: State, family: BTreeSet<String>) -> Live {
        Live {
            rules,
            lease,
            watch: Watch::new(state, family.clone()),
            remarks: Remarks::new(),
            smoother: Smoother::new(),
            ui: Ui::new(family),
            scene: None,
            regions: Vec::new(),
            frame: None,
        }
    }

    /// Credits focus to the tenant owning `pid`; call once a second.
    pub fn focus(&mut self, pid: u32) {
        self.watch.focus(pid);
    }

    /// Groups `snapshot`, advances watched time and flags, picks the
    /// remark, lays out the smoothed building and draws the window with
    /// current numbers. Returns whether the frame changed.
    pub fn update(&mut self, snapshot: &Snapshot) -> Result<bool, BuildingError> {
        let grouping = group(snapshot, &self.rules);
        let gained = self.watch.observe(snapshot, &grouping);
        let pids = grouping
            .tenants
            .iter()
            .map(|t| (t.id.clone(), t.pids.clone()))
            .collect();
        let mut current = building_of(snapshot, grouping, &self.lease);
        for tenant in &mut current.tenants {
            tenant.flags = self.watch.flags_of(&tenant.id).to_vec();
        }
        let evicted = self.ui.sampled(&current);
        let sample = Sample {
            watched_ms: self.watch.state().watched_ms,
            commit_used_bytes: snapshot.commit_used_bytes,
            commit_limit_bytes: snapshot.commit_limit_bytes,
            signed_in_at_ms: snapshot.signed_in_at_ms,
            gained: &gained,
            evicted: evicted.as_deref(),
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
        self.scene = Some(Scene {
            building: current,
            layout,
            highlight,
            at_ms: snapshot.at_ms,
            pids,
        });
        Ok(self.redraw())
    }

    /// Draws the window from the last sample and the UI state. Returns
    /// whether the frame changed.
    fn redraw(&mut self) -> bool {
        let Some(scene) = &self.scene else {
            return false;
        };
        let selected = self.ui.selected_index(&scene.building);
        let details = match (self.ui.view, selected) {
            (View::Details, Some(tenant)) => Some(self.details(tenant)),
            _ => None,
        };
        let panel = match (&self.ui.eviction, &details) {
            (Eviction::Idle, Some(details)) => Panel::Details(details),
            (Eviction::Idle, None) if self.ui.view == View::Staff => Panel::Staff,
            (Eviction::Idle, None) => Panel::Roll,
            (eviction, _) => Panel::Dialog(eviction),
        };
        let (draws, regions) = scene::window(
            &scene.building,
            &scene.layout,
            scene.highlight,
            selected,
            &panel,
        );
        let frame = rasterize(scene::WIDTH, scene::HEIGHT, SKY, &draws);
        let changed = self.frame.as_ref() != Some(&frame);
        self.frame = Some(frame);
        self.regions = regions;
        changed
    }

    /// What the details card shows about tenant `index`.
    fn details(&self, index: usize) -> Details {
        let scene = self.scene.as_ref().expect("a sample was drawn");
        let tenant = &scene.building.tenants[index];
        let instance = self.watch.record(&tenant.id).and_then(|r| r.live.as_ref());
        let buckets = instance.map_or(&[][..], |i| i.buckets.as_slice());
        Details {
            tenant: index,
            peak_bytes: instance.map_or(tenant.rent_bytes, |i| i.peak_bytes),
            hour_ago_bytes: (buckets.len() == BUCKETS + 1).then(|| buckets[0]),
            moved_in_ms: instance
                .filter(|i| i.started_at_ms > 0)
                .map(|i| scene.at_ms.saturating_sub(i.started_at_ms)),
            last_visit_ms: instance.filter(|i| i.ever_focused).map(|i| i.unfocused_ms),
            history: buckets.iter().skip(1).copied().collect(),
            family: self.ui.family.contains(&tenant.id),
        }
    }

    /// Handles a click (in canvas pixels) or a key. Returns the commands for
    /// the Windows layer; the frame is redrawn.
    pub fn input(&mut self, input: Input, now_ms: u64) -> Vec<Command> {
        let Some(scene) = &self.scene else {
            return Vec::new();
        };
        let rows: Vec<usize> = Roll::new(&scene.building, &scene.layout)
            .rows
            .iter()
            .map(|r| r.tenant)
            .collect();
        let cx = Context {
            building: &scene.building,
            rows: &rows,
            regions: &self.regions,
            pids: &scene.pids,
            now_ms,
        };
        let commands = self.ui.input(input, &cx);
        for command in &commands {
            if let Command::SaveFamily(family) = command {
                self.watch.set_family(family.clone());
            }
        }
        self.redraw();
        commands
    }

    /// Which of the evicted processes remain; call while `waiting()`.
    pub fn poll(&mut self, remaining: &[u32], now_ms: u64) -> bool {
        self.ui.poll(remaining, now_ms);
        self.redraw()
    }

    /// How a Close or Terminate went.
    pub fn report(&mut self, report: Report) -> bool {
        self.ui.report(report);
        self.redraw()
    }

    /// Whether an eviction is waiting for processes to exit.
    pub fn waiting(&self) -> bool {
        self.ui.waiting()
    }

    /// The processes an eviction is waiting for.
    pub fn waiting_on(&self) -> &[u32] {
        self.ui.waiting_on()
    }

    /// The last frame drawn.
    pub fn frame(&self) -> Option<&Frame> {
        self.frame.as_ref()
    }

    /// The last building, with current numbers, flags and the remark.
    pub fn building(&self) -> Option<&Building> {
        self.scene.as_ref().map(|s| &s.building)
    }

    /// The selection, the view and the eviction.
    pub fn ui(&self) -> &Ui {
        &self.ui
    }

    /// The clickable regions of the last frame.
    pub fn regions(&self) -> &[Region] {
        &self.regions
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
