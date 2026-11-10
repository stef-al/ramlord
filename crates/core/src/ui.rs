//! Selection, the details card and eviction (SPEC.md, Input; Evicting a
//! tenant). A pure state machine: input and reports from the Windows layer
//! in, commands for it out. Time arrives with every call.
//!
//! Eviction asks politely first and never happens on its own:
//!
//! ```text
//! request ─► Confirm ─yes─► Asking ─all left─► Gone ─next sample─► Done
//!               │              │ 5 s, some stay
//!               │ (walls)      ▼
//!               └────────► Refused ─force─► Forcing ─all left─► Gone
//!                              │ leave         │ denied
//!                              ▼               ▼
//!                             Idle           Lawyer
//! ```

use std::collections::{BTreeMap, BTreeSet};

use crate::building::{Building, Flag};
use crate::format::{cut, wrap_balanced};
use crate::remarks::evicted_text;
use crate::settings::{ROWS, Row, Settings};

/// How long the polite phase waits for the processes to exit.
pub const POLITE_MS: u64 = 5_000;
/// Names on the details card and in eviction messages.
pub const NAME_CHARS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Enter,
    Delete,
    Escape,
    F2,
    F3,
    Backspace,
}

/// Something the user clicked: a region of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A tenant's unit, box or rent roll row (an index into
    /// `Building::tenants`).
    Tenant(usize),
    /// The `F2 RENT ROLL` hint.
    RentRollCard,
    /// The `F3 SETTINGS` hint.
    Settings,
    /// A row of the settings screen, by its place in `settings::ROWS`.
    Setting(u8),
    /// Staff's units or its rent roll row: the Staff card.
    Staff,
    /// Others' units or its rent roll row: the Others card.
    Others,
    /// The Others card's next page.
    More,
    Evict,
    Family,
    Startup,
    Back,
    /// Dialog buttons.
    CloseIt,
    Cancel,
    Force,
    Leave,
    Ok,
}

/// A clickable rectangle on the 384 by 240 canvas, corners inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    pub target: Target,
}

/// The region under a point; later regions lie on top.
pub fn hit(regions: &[Region], x: i32, y: i32) -> Option<Target> {
    regions
        .iter()
        .rev()
        .find(|r| r.x0 <= x && x <= r.x1 && r.y0 <= y && y <= r.y1)
        .map(|r| r.target)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// A click at a canvas pixel.
    Click {
        x: i32,
        y: i32,
    },
    Key(Key),
    /// A typed character, for the custom price.
    Char(char),
}

/// What the Windows layer is asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Post WM_CLOSE to every visible top-level window of these processes,
    /// then report `Posted` (or `Denied`) and start reporting which remain.
    Close { pids: Vec<u32> },
    /// TerminateProcess on each, then report `Denied` if Windows refused.
    Terminate { pids: Vec<u32> },
    /// Launch `ms-settings:startupapps`.
    OpenStartupSettings,
    /// F2: save the rent roll card and copy it to the clipboard.
    RentRollCard,
    /// The Family list changed. `Live` turns it into `ApplySettings`.
    SaveFamily(BTreeSet<String>),
    /// Enter on a settings row. `Live` turns it into `ApplySettings`.
    Setting(Row),
    /// A typed price, to read. `Live` turns it into `ApplySettings`.
    Price(String),
    /// The settings changed: save them and apply what is the Windows
    /// layer's (scale, on top, start with Windows, price fetching).
    ApplySettings(Settings),
}

/// What the Windows layer reports back during an eviction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Report {
    /// WM_CLOSE went to this many windows.
    Posted(usize),
    /// Windows refused: the app runs with higher rights.
    Denied,
}

/// Where an eviction stands. `tenant` is the tenant's id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Eviction {
    Idle,
    /// "Close NAME? Unsaved work may be lost if it does not ask first."
    Confirm {
        tenant: String,
        name: String,
        before: u64,
    },
    /// WM_CLOSE posted; waiting up to 5 seconds.
    Asking {
        tenant: String,
        name: String,
        pids: Vec<u32>,
        until_ms: u64,
        before: u64,
    },
    /// "NAME refuses to leave." (or has no window to ask): Force or Leave.
    Refused {
        tenant: String,
        name: String,
        pids: Vec<u32>,
        before: u64,
        no_window: bool,
    },
    /// TerminateProcess requested.
    Forcing {
        tenant: String,
        name: String,
        pids: Vec<u32>,
        before: u64,
    },
    /// "NAME has a lawyer. Close it yourself."
    Lawyer {
        name: String,
    },
    /// Every process has left. The next sample closes the dialog and the
    /// landlord's remark says what it vacated.
    Gone {
        tenant: String,
        name: String,
        before: u64,
    },
}

impl Eviction {
    /// The dialog's message.
    pub fn message(&self) -> Option<String> {
        Some(match self {
            Eviction::Idle => return None,
            Eviction::Confirm { name, .. } => {
                format!("Close {name}? Unsaved work may be lost if it does not ask first.")
            }
            Eviction::Asking { name, .. } => format!("Asking {name} to leave..."),
            Eviction::Refused {
                name, no_window, ..
            } => {
                if *no_window {
                    format!("{name} lives in the walls. There is no window to ask.")
                } else {
                    format!("{name} refuses to leave.")
                }
            }
            Eviction::Forcing { name, .. } => format!("Evicting {name}..."),
            Eviction::Lawyer { name } => format!("{name} has a lawyer. Close it yourself."),
            Eviction::Gone { name, .. } => format!("{name} has left. Counting the rooms..."),
        })
    }

    /// The message wrapped for the panel.
    pub fn lines(&self, width: usize) -> Vec<String> {
        self.message()
            .map(|m| wrap_balanced(&m.to_uppercase(), width))
            .unwrap_or_default()
    }

    /// The dialog's buttons, primary (Enter) first.
    /// The keys the dialog answers, as its hint line says them.
    pub fn keys(&self) -> &'static str {
        match self {
            Eviction::Refused { .. } => "ESC LEAVES, DEL FORCES",
            Eviction::Confirm { .. } | Eviction::Lawyer { .. } => "ENTER OR ESC",
            _ => "",
        }
    }

    pub fn buttons(&self) -> &'static [Target] {
        match self {
            Eviction::Confirm { .. } => &[Target::CloseIt, Target::Cancel],
            Eviction::Refused { .. } => &[Target::Leave, Target::Force],
            Eviction::Lawyer { .. } => &[Target::Ok],
            _ => &[],
        }
    }
}

/// Which panel shows when no eviction dialog is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Roll,
    Details,
    /// The Staff card: what Staff is made of.
    Staff,
    /// The Others card: everyone the rent roll does not list.
    Others,
    /// The settings screen (F3).
    Settings,
}

/// What the state machine needs to know about the moment.
pub struct Context<'a> {
    pub building: &'a Building,
    /// Tenants in rent roll order (indices into `building.tenants`).
    pub rows: &'a [usize],
    /// The Others card's tenants, in its order.
    pub others: &'a [usize],
    /// The clickable regions of the frame on screen.
    pub regions: &'a [Region],
    /// Each tenant's processes, by tenant id.
    pub pids: &'a BTreeMap<String, Vec<u32>>,
    pub now_ms: u64,
}

pub struct Ui {
    pub view: View,
    /// The selected tenant's id.
    pub selected: Option<String>,
    pub eviction: Eviction,
    pub family: BTreeSet<String>,
    /// Where the details card goes back to: the rent roll, or the Others
    /// card it was opened from.
    pub back_to: View,
    /// The selected row of the settings screen.
    pub settings_at: usize,
    /// The price being typed, while it is.
    pub editing: Option<String>,
}

/// The most characters a typed price may have.
const PRICE_CHARS: usize = 12;

/// Rows on one page of the Others card.
pub const OTHERS_PAGE: usize = 13;

impl Ui {
    pub fn new(family: BTreeSet<String>) -> Ui {
        Ui {
            view: View::Roll,
            selected: None,
            eviction: Eviction::Idle,
            family,
            back_to: View::Roll,
            settings_at: 0,
            editing: None,
        }
    }

    /// The selected tenant's index in `building`, if it is still there.
    pub fn selected_index(&self, building: &Building) -> Option<usize> {
        let id = self.selected.as_deref()?;
        building.tenants.iter().position(|t| t.id == id)
    }

    /// Forgets a selection whose tenant has gone. After an eviction,
    /// closes the dialog and returns the report for the landlord's
    /// remark: what this sample shows was vacated, never a promise.
    pub fn sampled(&mut self, building: &Building) -> Option<String> {
        let report = match &self.eviction {
            Eviction::Gone {
                tenant,
                name,
                before,
            } => {
                let left = building
                    .tenants
                    .iter()
                    .find(|t| &t.id == tenant)
                    .map_or(0, |t| t.rent_bytes);
                let report = evicted_text(name, before.saturating_sub(left));
                self.eviction = Eviction::Idle;
                Some(report)
            }
            _ => None,
        };
        if self.selected_index(building).is_none() {
            self.selected = None;
            if self.view == View::Details {
                self.view = self.back_to;
            }
        }
        report
    }

    fn select(&mut self, building: &Building, index: usize, view: View) {
        self.selected = Some(building.tenants[index].id.clone());
        self.view = view;
    }

    /// Starts an eviction of `index`. Family cannot be evicted; Staff and
    /// RAMlord itself are never tenants.
    fn request(&mut self, cx: &Context, index: usize) {
        let tenant = &cx.building.tenants[index];
        if tenant.has(Flag::Family) || self.family.contains(&tenant.id) {
            return;
        }
        self.eviction = Eviction::Confirm {
            tenant: tenant.id.clone(),
            name: cut(&tenant.name.to_uppercase(), NAME_CHARS),
            before: tenant.rent_bytes,
        };
    }

    /// Answers a dialog button.
    fn answer(&mut self, cx: &Context, button: Target) -> Vec<Command> {
        let eviction = std::mem::replace(&mut self.eviction, Eviction::Idle);
        match (eviction, button) {
            (
                Eviction::Confirm {
                    tenant,
                    name,
                    before,
                },
                Target::CloseIt,
            ) => {
                let pids = cx.pids.get(&tenant).cloned().unwrap_or_default();
                let walls = cx
                    .building
                    .tenants
                    .iter()
                    .find(|t| t.id == tenant)
                    .is_some_and(|t| t.has(Flag::Walls));
                if walls {
                    // A tenant in the walls has no window to ask.
                    self.eviction = Eviction::Refused {
                        tenant,
                        name,
                        pids,
                        before,
                        no_window: true,
                    };
                    return Vec::new();
                }
                self.eviction = Eviction::Asking {
                    tenant,
                    name,
                    pids: pids.clone(),
                    until_ms: cx.now_ms + POLITE_MS,
                    before,
                };
                vec![Command::Close { pids }]
            }
            (
                Eviction::Refused {
                    tenant,
                    name,
                    pids,
                    before,
                    ..
                },
                Target::Force,
            ) => {
                self.eviction = Eviction::Forcing {
                    tenant,
                    name,
                    pids: pids.clone(),
                    before,
                };
                vec![Command::Terminate { pids }]
            }
            (_, Target::Cancel | Target::Leave | Target::Ok) => Vec::new(),
            (other, _) => {
                // A button the dialog does not have: nothing changes.
                self.eviction = other;
                Vec::new()
            }
        }
    }

    /// The settings screen's keys and clicks. None: not for it (a click on
    /// the tower, say), handled as anywhere else.
    fn settings_input(&mut self, input: Input, cx: &Context) -> Option<Vec<Command>> {
        if let Some(text) = &mut self.editing {
            match input {
                Input::Char(c) if !c.is_control() && text.chars().count() < PRICE_CHARS => {
                    text.push(c);
                }
                Input::Key(Key::Backspace) => {
                    text.pop();
                }
                Input::Key(Key::Enter) => {
                    let text = self.editing.take().unwrap_or_default();
                    return Some(vec![Command::Price(text)]);
                }
                Input::Key(Key::Escape) => self.editing = None,
                Input::Click { .. } => {
                    // A click elsewhere ends the typing, unsaved.
                    self.editing = None;
                    return None;
                }
                _ => {}
            }
            return Some(Vec::new());
        }
        let last = ROWS.len() - 1;
        let enter = |ui: &mut Ui, at: usize| -> Vec<Command> {
            ui.settings_at = at;
            match ROWS[at] {
                Row::Price => {
                    ui.editing = Some(String::new());
                    Vec::new()
                }
                row => vec![Command::Setting(row)],
            }
        };
        Some(match input {
            Input::Key(Key::Up) => {
                self.settings_at = self.settings_at.saturating_sub(1);
                Vec::new()
            }
            Input::Key(Key::Down) => {
                self.settings_at = (self.settings_at + 1).min(last);
                Vec::new()
            }
            Input::Key(Key::Enter) => enter(self, self.settings_at),
            Input::Key(Key::Escape | Key::F3) => {
                self.view = View::Roll;
                Vec::new()
            }
            Input::Key(Key::F2) => vec![Command::RentRollCard],
            Input::Click { x, y } => match hit(cx.regions, x, y) {
                Some(Target::Setting(at)) => enter(self, usize::from(at).min(last)),
                Some(Target::Back) => {
                    self.view = View::Roll;
                    Vec::new()
                }
                _ => return None,
            },
            _ => Vec::new(),
        })
    }

    /// Handles a click or a key.
    pub fn input(&mut self, input: Input, cx: &Context) -> Vec<Command> {
        // An open dialog takes every input.
        if self.eviction == Eviction::Idle
            && self.view == View::Settings
            && let Some(commands) = self.settings_input(input, cx)
        {
            return commands;
        }
        let selected = self.selected_index(cx.building);
        if self.eviction != Eviction::Idle {
            let buttons = self.eviction.buttons();
            let button = match input {
                Input::Click { x, y } => hit(cx.regions, x, y).filter(|t| buttons.contains(t)),
                Input::Key(Key::Enter) => buttons.first().copied(),
                // Esc is always the way out that changes nothing.
                Input::Key(Key::Escape) => buttons
                    .iter()
                    .copied()
                    .find(|t| matches!(t, Target::Cancel | Target::Leave | Target::Ok)),
                Input::Key(Key::Delete) if buttons.contains(&Target::Force) => Some(Target::Force),
                Input::Key(_) | Input::Char(_) => None,
            };
            return match button {
                Some(button) => self.answer(cx, button),
                None => Vec::new(),
            };
        }

        match input {
            Input::Click { x, y } => match hit(cx.regions, x, y) {
                Some(Target::Tenant(t)) => {
                    // From the Others card, Back returns to it.
                    self.back_to = match self.view {
                        View::Others => View::Others,
                        _ => View::Roll,
                    };
                    self.select(cx.building, t, View::Details);
                    Vec::new()
                }
                Some(Target::Staff) => {
                    self.selected = None;
                    self.view = View::Staff;
                    Vec::new()
                }
                Some(Target::Others) => {
                    self.selected = None;
                    self.view = View::Others;
                    Vec::new()
                }
                Some(Target::More) => {
                    // The first tenant of the next page, round to the first.
                    let at = selected
                        .and_then(|s| cx.others.iter().position(|&t| t == s))
                        .unwrap_or(0);
                    let next = (at / OTHERS_PAGE + 1) * OTHERS_PAGE;
                    let next = if next < cx.others.len() { next } else { 0 };
                    if let Some(&t) = cx.others.get(next) {
                        self.select(cx.building, t, View::Others);
                    }
                    Vec::new()
                }
                Some(Target::RentRollCard) => vec![Command::RentRollCard],
                Some(Target::Settings) => {
                    self.view = View::Settings;
                    Vec::new()
                }
                Some(Target::Back) => {
                    self.view = match self.view {
                        View::Details => self.back_to,
                        _ => View::Roll,
                    };
                    Vec::new()
                }
                Some(Target::Evict) => {
                    if let Some(t) = selected {
                        self.request(cx, t);
                    }
                    Vec::new()
                }
                Some(Target::Family) => self.toggle_family(cx),
                Some(Target::Startup) => vec![Command::OpenStartupSettings],
                _ => Vec::new(),
            },
            Input::Key(key) => match key {
                Key::Up | Key::Down => {
                    // On the Others card, and on a card opened from it, the
                    // keys move through its list; elsewhere, the rent roll.
                    let in_others = self.view == View::Others
                        || (self.view == View::Details && self.back_to == View::Others);
                    let rows = if in_others { cx.others } else { cx.rows };
                    if rows.is_empty() {
                        return Vec::new();
                    }
                    let at = selected.and_then(|s| rows.iter().position(|&r| r == s));
                    let last = rows.len() - 1;
                    let next = match (key == Key::Down, at) {
                        (true, None) => 0,
                        (false, None) => last,
                        (true, Some(k)) => (k + 1).min(last),
                        (false, Some(k)) => k.saturating_sub(1),
                    };
                    // From the Staff card, a row takes the rent roll back.
                    let view = match self.view {
                        View::Staff => View::Roll,
                        view => view,
                    };
                    self.select(cx.building, rows[next], view);
                    Vec::new()
                }
                Key::Enter => {
                    if selected.is_some() && self.view != View::Details {
                        self.back_to = match self.view {
                            View::Others => View::Others,
                            _ => View::Roll,
                        };
                        self.view = View::Details;
                    }
                    Vec::new()
                }
                Key::Delete => {
                    if let Some(t) = selected {
                        self.request(cx, t);
                    }
                    Vec::new()
                }
                Key::Escape => {
                    match self.view {
                        View::Roll => self.selected = None,
                        View::Details => self.view = self.back_to,
                        _ => self.view = View::Roll,
                    }
                    Vec::new()
                }
                Key::F2 => vec![Command::RentRollCard],
                Key::F3 => {
                    self.view = View::Settings;
                    Vec::new()
                }
                Key::Backspace => Vec::new(),
            },
            Input::Char(_) => Vec::new(),
        }
    }

    fn toggle_family(&mut self, cx: &Context) -> Vec<Command> {
        let Some(t) = self.selected_index(cx.building) else {
            return Vec::new();
        };
        let id = cx.building.tenants[t].id.clone();
        if !self.family.remove(&id) {
            self.family.insert(id);
        }
        vec![Command::SaveFamily(self.family.clone())]
    }

    /// The Windows layer reports which of the evicted processes remain.
    pub fn poll(&mut self, remaining: &[u32], now_ms: u64) {
        let next = match &self.eviction {
            Eviction::Asking {
                tenant,
                name,
                until_ms,
                before,
                ..
            } => {
                if remaining.is_empty() {
                    Some(Eviction::Gone {
                        tenant: tenant.clone(),
                        name: name.clone(),
                        before: *before,
                    })
                } else if now_ms >= *until_ms {
                    Some(Eviction::Refused {
                        tenant: tenant.clone(),
                        name: name.clone(),
                        pids: remaining.to_vec(),
                        before: *before,
                        no_window: false,
                    })
                } else {
                    None
                }
            }
            Eviction::Forcing {
                tenant,
                name,
                before,
                ..
            } if remaining.is_empty() => Some(Eviction::Gone {
                tenant: tenant.clone(),
                name: name.clone(),
                before: *before,
            }),
            _ => None,
        };
        if let Some(next) = next {
            self.eviction = next;
        }
    }

    /// The Windows layer reports how a Close or Terminate went.
    pub fn report(&mut self, report: Report) {
        let name = match &self.eviction {
            Eviction::Asking { name, .. } | Eviction::Forcing { name, .. } => name.clone(),
            _ => return,
        };
        match report {
            Report::Denied => self.eviction = Eviction::Lawyer { name },
            Report::Posted(0) => {
                // No window to ask after all: straight to the Force question.
                if let Eviction::Asking {
                    tenant,
                    name,
                    pids,
                    before,
                    ..
                } = std::mem::replace(&mut self.eviction, Eviction::Idle)
                {
                    self.eviction = Eviction::Refused {
                        tenant,
                        name,
                        pids,
                        before,
                        no_window: true,
                    };
                }
            }
            Report::Posted(_) => {}
        }
    }

    /// Whether the Windows layer should poll the evicted processes.
    pub fn waiting(&self) -> bool {
        matches!(
            self.eviction,
            Eviction::Asking { .. } | Eviction::Forcing { .. }
        )
    }

    /// The pids an eviction is waiting on.
    pub fn waiting_on(&self) -> &[u32] {
        match &self.eviction {
            Eviction::Asking { pids, .. } | Eviction::Forcing { pids, .. } => pids,
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::test_building;

    const MIB: u64 = 1 << 20;

    struct World {
        building: Building,
        rows: Vec<usize>,
        regions: Vec<Region>,
        pids: BTreeMap<String, Vec<u32>>,
    }

    /// Three tenants; Discord lives in the walls. Rows in that order.
    fn world() -> World {
        let mut building = test_building(
            1000 * MIB,
            &[
                ("Chrome", 3000 * MIB),
                ("Discord", 2970 * MIB),
                ("Teams", 1000 * MIB),
            ],
            1222 * MIB,
        );
        building.tenants[1].flags = vec![Flag::Walls];
        let pids = [
            ("chrome", vec![10, 11]),
            ("discord", vec![20]),
            ("teams", vec![30]),
        ]
        .into_iter()
        .map(|(id, p)| (id.to_string(), p))
        .collect();
        let regions = vec![
            Region {
                x0: 0,
                y0: 0,
                x1: 9,
                y1: 9,
                target: Target::Tenant(0),
            },
            Region {
                x0: 10,
                y0: 0,
                x1: 19,
                y1: 9,
                target: Target::Tenant(2),
            },
            Region {
                x0: 0,
                y0: 50,
                x1: 9,
                y1: 59,
                target: Target::CloseIt,
            },
            Region {
                x0: 10,
                y0: 50,
                x1: 19,
                y1: 59,
                target: Target::Cancel,
            },
            Region {
                x0: 0,
                y0: 60,
                x1: 9,
                y1: 69,
                target: Target::Force,
            },
            Region {
                x0: 0,
                y0: 70,
                x1: 9,
                y1: 79,
                target: Target::Family,
            },
            Region {
                x0: 0,
                y0: 80,
                x1: 9,
                y1: 89,
                target: Target::RentRollCard,
            },
        ];
        World {
            building,
            rows: vec![0, 1, 2],
            regions,
            pids,
        }
    }

    impl World {
        fn cx(&self, now_ms: u64) -> Context<'_> {
            Context {
                building: &self.building,
                rows: &self.rows,
                others: &[],
                regions: &self.regions,
                pids: &self.pids,
                now_ms,
            }
        }
    }

    fn key(ui: &mut Ui, w: &World, k: Key) -> Vec<Command> {
        ui.input(Input::Key(k), &w.cx(0))
    }

    #[test]
    fn keys_move_the_selection_and_open_the_card() {
        let w = world();
        let mut ui = Ui::new(BTreeSet::new());
        key(&mut ui, &w, Key::Down);
        assert_eq!(ui.selected.as_deref(), Some("chrome"));
        key(&mut ui, &w, Key::Down);
        key(&mut ui, &w, Key::Down);
        key(&mut ui, &w, Key::Down);
        assert_eq!(ui.selected.as_deref(), Some("teams"));
        key(&mut ui, &w, Key::Up);
        assert_eq!(ui.selected.as_deref(), Some("discord"));
        assert_eq!(ui.view, View::Roll);
        key(&mut ui, &w, Key::Enter);
        assert_eq!(ui.view, View::Details);
        key(&mut ui, &w, Key::Escape);
        assert_eq!((ui.view, ui.selected.is_some()), (View::Roll, true));
        key(&mut ui, &w, Key::Escape);
        assert_eq!(ui.selected, None);
        assert_eq!(key(&mut ui, &w, Key::F2), [Command::RentRollCard]);
        assert!(key(&mut ui, &w, Key::F3).is_empty());
        assert_eq!(ui.view, View::Settings);
    }

    #[test]
    fn clicks_select_a_tenant_and_reach_the_hints() {
        let w = world();
        let mut ui = Ui::new(BTreeSet::new());
        ui.input(Input::Click { x: 15, y: 5 }, &w.cx(0));
        assert_eq!(
            (ui.selected.as_deref(), ui.view),
            (Some("teams"), View::Details)
        );
        assert_eq!(
            ui.input(Input::Click { x: 5, y: 85 }, &w.cx(0)),
            [Command::RentRollCard]
        );
        // A click on nothing changes nothing.
        ui.input(Input::Click { x: 300, y: 300 }, &w.cx(0));
        assert_eq!(ui.selected.as_deref(), Some("teams"));
    }

    #[test]
    fn a_polite_exit() {
        let w = world();
        let mut ui = Ui::new(BTreeSet::new());
        key(&mut ui, &w, Key::Down); // Chrome
        assert!(key(&mut ui, &w, Key::Delete).is_empty());
        assert!(matches!(ui.eviction, Eviction::Confirm { .. }));
        assert_eq!(
            ui.eviction.message().unwrap(),
            "Close CHROME? Unsaved work may be lost if it does not ask first."
        );
        let commands = ui.input(Input::Key(Key::Enter), &w.cx(1_000));
        assert_eq!(commands, [Command::Close { pids: vec![10, 11] }]);
        ui.report(Report::Posted(2));
        ui.poll(&[11], 3_000);
        assert!(matches!(ui.eviction, Eviction::Asking { .. }));
        ui.poll(&[], 4_000);
        assert!(matches!(ui.eviction, Eviction::Gone { .. }));
        // The next sample without Chrome says how much was vacated; nothing
        // is promised before. The dialog closes and the rent roll returns.
        let mut after = w.building.clone();
        after.tenants.remove(0);
        assert_eq!(
            ui.sampled(&after).as_deref(),
            Some("Evicted CHROME. 2.9 GB vacated.")
        );
        assert_eq!(ui.eviction, Eviction::Idle);
        assert_eq!(ui.selected, None);
        assert_eq!(ui.view, View::Roll);
    }

    #[test]
    fn a_refusal_then_leave_it() {
        let w = world();
        let mut ui = Ui::new(BTreeSet::new());
        ui.input(Input::Click { x: 5, y: 5 }, &w.cx(0)); // Chrome
        key(&mut ui, &w, Key::Delete);
        ui.input(Input::Click { x: 5, y: 55 }, &w.cx(1_000)); // Close it
        ui.report(Report::Posted(1));
        ui.poll(&[10, 11], 5_999);
        assert!(matches!(ui.eviction, Eviction::Asking { .. }));
        ui.poll(&[10], 6_000);
        assert_eq!(ui.eviction.message().unwrap(), "CHROME refuses to leave.");
        assert_eq!(ui.eviction.buttons(), [Target::Leave, Target::Force]);
        // Enter is the safe choice: leave it.
        assert!(key(&mut ui, &w, Key::Enter).is_empty());
        assert_eq!(ui.eviction, Eviction::Idle);
    }

    #[test]
    fn a_forced_exit() {
        let w = world();
        let mut ui = Ui::new(BTreeSet::new());
        key(&mut ui, &w, Key::Down);
        key(&mut ui, &w, Key::Delete);
        key(&mut ui, &w, Key::Enter);
        ui.poll(&[11], 6_000);
        // Delete, or a click on Force evict, terminates what remains.
        let commands = ui.input(Input::Click { x: 5, y: 65 }, &w.cx(6_100));
        assert_eq!(commands, [Command::Terminate { pids: vec![11] }]);
        ui.poll(&[], 6_200);
        assert!(matches!(ui.eviction, Eviction::Gone { .. }));
    }

    #[test]
    fn a_denied_one_has_a_lawyer() {
        let w = world();
        let mut ui = Ui::new(BTreeSet::new());
        key(&mut ui, &w, Key::Down);
        key(&mut ui, &w, Key::Delete);
        key(&mut ui, &w, Key::Enter);
        ui.poll(&[10], 6_000);
        assert_eq!(
            key(&mut ui, &w, Key::Delete),
            [Command::Terminate { pids: vec![10] }]
        );
        ui.report(Report::Denied);
        assert_eq!(
            ui.eviction.message().unwrap(),
            "CHROME has a lawyer. Close it yourself."
        );
        key(&mut ui, &w, Key::Escape);
        assert_eq!(ui.eviction, Eviction::Idle);
        // A refused WM_CLOSE is a lawyer too.
        key(&mut ui, &w, Key::Delete);
        key(&mut ui, &w, Key::Enter);
        ui.report(Report::Denied);
        assert!(matches!(ui.eviction, Eviction::Lawyer { .. }));
    }

    #[test]
    fn a_tenant_in_the_walls_goes_straight_to_the_force_question() {
        let w = world();
        let mut ui = Ui::new(BTreeSet::new());
        key(&mut ui, &w, Key::Down);
        key(&mut ui, &w, Key::Down); // Discord, in the walls
        key(&mut ui, &w, Key::Delete);
        assert!(key(&mut ui, &w, Key::Enter).is_empty(), "no window to ask");
        assert_eq!(
            ui.eviction.message().unwrap(),
            "DISCORD lives in the walls. There is no window to ask."
        );
        assert_eq!(
            key(&mut ui, &w, Key::Delete),
            [Command::Terminate { pids: vec![20] }]
        );
        // A tenant whose windows turn out to be none goes the same way.
        let mut ui = Ui::new(BTreeSet::new());
        key(&mut ui, &w, Key::Down);
        key(&mut ui, &w, Key::Delete);
        key(&mut ui, &w, Key::Enter);
        ui.report(Report::Posted(0));
        assert!(matches!(
            ui.eviction,
            Eviction::Refused {
                no_window: true,
                ..
            }
        ));
    }

    #[test]
    fn cancel_and_family_are_safe() {
        let w = world();
        let mut ui = Ui::new(BTreeSet::new());
        key(&mut ui, &w, Key::Down);
        key(&mut ui, &w, Key::Delete);
        assert!(key(&mut ui, &w, Key::Escape).is_empty());
        assert_eq!(ui.eviction, Eviction::Idle);
        // Esc never forces: at the refusal it leaves the tenant be.
        key(&mut ui, &w, Key::Delete);
        key(&mut ui, &w, Key::Enter);
        ui.poll(&[10], 6_000);
        assert!(matches!(ui.eviction, Eviction::Refused { .. }));
        assert!(key(&mut ui, &w, Key::Escape).is_empty());
        assert_eq!(ui.eviction, Eviction::Idle);
        // Enter leaves it be too; only Delete or the button forces.
        key(&mut ui, &w, Key::Delete);
        key(&mut ui, &w, Key::Enter);
        ui.poll(&[10], 6_000);
        assert!(key(&mut ui, &w, Key::Enter).is_empty());
        assert_eq!(ui.eviction, Eviction::Idle);
        // Family on: Delete does nothing; off again: it asks.
        ui.input(Input::Click { x: 5, y: 5 }, &w.cx(0));
        let saved = ui.input(Input::Click { x: 5, y: 75 }, &w.cx(0));
        assert_eq!(saved, [Command::SaveFamily(["chrome".to_string()].into())]);
        key(&mut ui, &w, Key::Delete);
        assert_eq!(ui.eviction, Eviction::Idle);
        ui.input(Input::Click { x: 5, y: 75 }, &w.cx(0));
        key(&mut ui, &w, Key::Delete);
        assert!(matches!(ui.eviction, Eviction::Confirm { .. }));
    }

    #[test]
    fn messages_wrap_for_the_panel() {
        let e = Eviction::Confirm {
            tenant: "x".into(),
            name: "VISUAL STUDIO CO".into(),
            before: 0,
        };
        let lines = e.lines(24);
        assert!(lines.iter().all(|l| l.len() <= 24), "{lines:?}");
        assert_eq!(lines.join(" "), e.message().unwrap().to_uppercase());
    }
}
