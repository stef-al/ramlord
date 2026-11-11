//! The running tower: one snapshot after another in, a frame out. The
//! Windows layer feeds it a snapshot every 2 seconds and the foreground
//! window every second, passes on clicks and keys, and runs the commands
//! they produce; `--self-test` drives it off screen. Between snapshots it
//! keeps watched time, flags and hues (the state file), the remark, the
//! smoother, and the selection, details card and eviction (`ui`).

use std::collections::{BTreeMap, BTreeSet};

use crate::building::{Building, BuildingError};
use crate::flags::{BUCKETS, State, Watch};
use crate::format::price_text;
use crate::layout::Layout;
use crate::palette::SKY;
use crate::prices::{self, PriceTable, billed_type};
use crate::remarks::{Kind, Remarks, Sample};
use crate::render::{Frame, rasterize};
use crate::rules::Rules;
use crate::scene::{self, Details, Panel, Roll, SettingsView};
use crate::settings::{ROWS, Settings, parse_custom_price};
use crate::smbios::MemoryType;
use crate::smooth::Smoother;
use crate::snapshot::Snapshot;
use crate::tenants::{Lease, building_of, group};
use crate::ui::{Command, Context, Eviction, Input, ProcessKey, Region, Report, Ui, View};

/// The last sample, laid out, kept so input can be answered between samples.
struct Scene {
    building: Building,
    layout: Layout,
    highlight: Option<usize>,
    at_ms: u64,
    /// Each tenant's processes, by tenant id, each with its start time.
    processes: BTreeMap<String, Vec<ProcessKey>>,
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
    settings: Settings,
    prices: PriceTable,
    detected: Option<MemoryType>,
    /// A reply to show on the remark line at the next sample.
    notice: Option<String>,
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
            ui: Ui::new(family.clone()),
            scene: None,
            regions: Vec::new(),
            frame: None,
            settings: Settings {
                family,
                ..Settings::default()
            },
            prices: PriceTable::embedded(),
            detected: None,
            notice: None,
        }
    }

    /// Gives the building its settings, price file and the memory type the
    /// PC reports (M5): the lease, the AWAY threshold and Family follow.
    /// When the type is neither detected nor chosen, the landlord asks.
    pub fn configure(
        &mut self,
        settings: Settings,
        prices: PriceTable,
        detected: Option<MemoryType>,
    ) {
        self.settings = settings;
        self.prices = prices;
        self.detected = detected;
        self.apply_settings();
        if billed_type(&self.settings, detected).is_none() {
            self.notice("What RAM is this? Pick its type in settings: F3.");
        }
    }

    /// A newer price file, fetched by the Windows layer.
    pub fn set_prices(&mut self, prices: PriceTable) -> bool {
        self.prices = prices;
        self.apply_settings();
        self.redraw()
    }

    /// Shows `text` on the remark line from the next sample on, at once.
    pub fn notice(&mut self, text: impl Into<String>) {
        self.notice = Some(text.into());
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The price table bills are made from.
    pub fn prices(&self) -> &PriceTable {
        &self.prices
    }

    /// Notes when prices were last fetched or tried, to try once a day.
    pub fn mark_prices_checked(&mut self, at_ms: u64) {
        self.settings.price_checked_ms = at_ms;
    }

    /// The lease, Family and AWAY threshold from the settings; the drawn
    /// building is billed anew at once.
    fn apply_settings(&mut self) {
        self.lease = prices::lease(&self.settings, self.detected, &self.prices);
        self.watch.set_family(self.settings.family.clone());
        self.ui.family = self.settings.family.clone();
        self.watch
            .set_away_ms(u64::from(self.settings.away_hours) * 3_600_000);
        if let Some(scene) = &mut self.scene {
            scene.building.memory_type = self.lease.memory_type.clone();
            scene.building.price = self.lease.price.clone();
        }
    }

    /// What the settings screen shows.
    fn settings_view(&self) -> SettingsView {
        let file = prices::lease(
            &Settings {
                custom_price: None,
                ..self.settings.clone()
            },
            self.detected,
            &self.prices,
        )
        .price;
        let file_price = price_text(file.per_gb_micros, &file.symbol);
        let at = self.ui.settings_at.min(ROWS.len() - 1);
        SettingsView {
            rows: ROWS
                .iter()
                .map(|&row| {
                    (
                        row.label().to_string(),
                        self.settings.value_text(row, self.detected, &file_price),
                    )
                })
                .collect(),
            at,
            editing: self.ui.editing.clone(),
            help: ROWS[at].help().map(String::from),
        }
    }

    /// The rent roll card of the last sample (F2), anonymous if the
    /// settings say so.
    pub fn card(&self) -> Option<Frame> {
        let scene = self.scene.as_ref()?;
        let draws = scene::card_with(&scene.building, &scene.layout, self.settings.anonymous);
        Some(rasterize(
            scene::CARD_WIDTH,
            scene::CARD_HEIGHT,
            SKY,
            &draws,
        ))
    }

    /// The tray icon's numbers: occupancy in percent, and whether the
    /// building is full (the landlord says so).
    pub fn tray(&self) -> Option<(u64, bool)> {
        let scene = self.scene.as_ref()?;
        let full = self.remarks.current().is_some_and(|r| r.kind == Kind::Full);
        Some((scene.building.occupancy_percent(), full))
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
        let started: BTreeMap<u32, u64> = snapshot
            .processes
            .iter()
            .map(|p| (p.pid, p.started_at_ms))
            .collect();
        let processes = grouping
            .tenants
            .iter()
            .map(|t| {
                let keys = t
                    .pids
                    .iter()
                    .map(|&pid| ProcessKey {
                        pid,
                        started_at_ms: started.get(&pid).copied().unwrap_or(0),
                    })
                    .collect();
                (t.id.clone(), keys)
            })
            .collect();
        let mut current = building_of(snapshot, grouping, &self.lease);
        for tenant in &mut current.tenants {
            tenant.flags = self.watch.flags_of(&tenant.id).to_vec();
        }
        let notice = self.ui.sampled(&current).or_else(|| self.notice.take());
        let sample = Sample {
            watched_ms: self.watch.state().watched_ms,
            commit_used_bytes: snapshot.commit_used_bytes,
            commit_limit_bytes: snapshot.commit_limit_bytes,
            signed_in_at_ms: snapshot.signed_in_at_ms,
            gained: &gained,
            notice: notice.as_deref(),
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
            processes,
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
        let settings = (self.ui.view == View::Settings).then(|| self.settings_view());
        let panel = match (&self.ui.eviction, &details) {
            (Eviction::Idle, _) if settings.is_some() => {
                Panel::Settings(settings.as_ref().expect("checked"))
            }
            (Eviction::Idle, Some(details)) => Panel::Details(details),
            (Eviction::Idle, None) if self.ui.view == View::Staff => Panel::Staff,
            (Eviction::Idle, None) if self.ui.view == View::Others => Panel::Others,
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
        let roll = Roll::new(&scene.building, &scene.layout);
        let rows: Vec<usize> = roll.rows.iter().map(|r| r.tenant).collect();
        let cx = Context {
            building: &scene.building,
            rows: &rows,
            others: &roll.rest,
            regions: &self.regions,
            processes: &scene.processes,
            now_ms,
        };
        let today = scene.building.date.clone();
        let commands = self.ui.input(input, &cx);
        // Settings changes are the core's to make; the Windows layer gets
        // the result to save and apply.
        let mut out = Vec::with_capacity(commands.len());
        for command in commands {
            let changed = match command {
                Command::SaveFamily(family) => Some(Settings {
                    family,
                    ..self.settings.clone()
                }),
                Command::Setting(row) => Some(self.settings.changed(row)),
                Command::Price(text) => {
                    match parse_custom_price(&text, &self.prices.symbol, &today) {
                        Some(custom_price) => Some(Settings {
                            custom_price,
                            ..self.settings.clone()
                        }),
                        None => {
                            self.notice("Type a price like 5.20 or EUR 5.20, symbol optional.");
                            None
                        }
                    }
                }
                other => {
                    out.push(other);
                    None
                }
            };
            if let Some(settings) = changed {
                self.settings = settings;
                self.apply_settings();
                out.push(Command::ApplySettings(self.settings.clone()));
            }
        }
        self.redraw();
        out
    }

    /// How many of the evicted processes remain; call while `waiting()`.
    pub fn poll(&mut self, remaining: usize, now_ms: u64) -> bool {
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
