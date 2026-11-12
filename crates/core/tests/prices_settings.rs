//! M5: prices, the settings screen, the rent roll card and the tray icon,
//! through the whole pipeline.

use std::collections::BTreeSet;
use std::path::Path;

use ramlord_core::flags::State;
use ramlord_core::layout::Layout;
use ramlord_core::live::Live;
use ramlord_core::palette::{GOLD, TRAY_CLEAR, TRAY_FULL};
use ramlord_core::prices::PriceTable;
use ramlord_core::render::Draw;
use ramlord_core::scene::{self, Panel};
use ramlord_core::settings::Settings;
use ramlord_core::smbios::MemoryType;
use ramlord_core::tenants::Lease;
use ramlord_core::ui::{Command, Input, Key, View};
use ramlord_core::{Building, HueBook, Rules, Snapshot, render_tray_icon};

fn snapshot(name: &str) -> Snapshot {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    Snapshot::from_json(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn live(detected: Option<MemoryType>, settings: Settings) -> (Live, u64) {
    let mut live = Live::new(
        Rules::builtin(),
        Lease::placeholder(),
        State::default(),
        BTreeSet::new(),
    );
    live.configure(settings, PriceTable::embedded(), detected);
    let s = snapshot("snapshot-desk.json");
    live.update(&s).unwrap();
    (live, s.at_ms)
}

fn texts(draws: &[Draw]) -> Vec<String> {
    draws
        .iter()
        .filter_map(|d| match d {
            Draw::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Every text of the tower window with `panel`, and of the card.
fn screen_and_card(b: &Building, panel: &Panel, anonymous: bool) -> (Vec<String>, Vec<String>) {
    let (layout, _) = Layout::new(b, &HueBook::new());
    let (draws, _) = scene::window(b, &layout, None, Some(0), panel);
    let card = scene::card_with(b, &layout, anonymous);
    (texts(&draws), texts(&card))
}

/// With `RAMLORD_PNG_DIR` set, writes `frame` there at `scale`, to look at.
fn save(frame: &ramlord_core::Frame, scale: u32, name: &str) {
    let Some(dir) = std::env::var_os("RAMLORD_PNG_DIR") else {
        return;
    };
    let frame = frame.scaled(scale);
    let file = std::fs::File::create(Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&frame.to_rgb8())
        .unwrap();
}

fn type_text(live: &mut Live, text: &str, now: u64) -> Vec<Command> {
    for c in text.chars() {
        live.input(Input::Char(c), now);
    }
    live.input(Input::Key(Key::Enter), now)
}

#[test]
fn a_custom_price_and_symbol_change_every_amount_on_screen_and_on_the_card() {
    let (mut live, now) = live(Some(MemoryType::Ddr5), Settings::default());
    live.input(Input::Key(Key::F3), now);
    assert_eq!(live.ui().view, View::Settings);
    for _ in 0..5 {
        live.input(Input::Key(Key::Down), now);
    }
    live.input(Input::Key(Key::Enter), now);
    assert_eq!(live.ui().editing.as_deref(), Some(""));
    for c in "€5.2".chars() {
        live.input(Input::Char(c), now);
    }
    save(live.frame().unwrap(), 3, "settings-typing.png");
    for _ in 0.."€5.2".chars().count() {
        live.input(Input::Key(Key::Backspace), now);
    }
    let commands = type_text(&mut live, "€5.20", now);
    save(live.frame().unwrap(), 3, "settings-custom.png");
    save(&live.card().unwrap(), 4, "card-euro.png");
    let [Command::ApplySettings(saved)] = &commands[..] else {
        panic!("{commands:?}");
    };
    let custom = saved.custom_price.clone().expect("a custom price");
    assert_eq!(
        (custom.per_gb_micros, custom.symbol.as_str()),
        (5_200_000, "€")
    );
    let b = live.building().unwrap().clone();
    assert_eq!(b.price.symbol, "€");
    let roll = Panel::Roll;
    let details = scene::Details {
        tenant: 0,
        ..Default::default()
    };
    for panel in [roll, Panel::Details(&details)] {
        let (screen, card) = screen_and_card(&b, &panel, false);
        for text in screen.iter().chain(&card) {
            assert!(!text.contains('$'), "{text}");
        }
        assert!(screen.iter().any(|t| t.contains('€')));
    }
    let (screen, card) = screen_and_card(&b, &Panel::Roll, false);
    assert!(
        screen.iter().any(|t| t.starts_with("DDR5 €5.20/GB")),
        "{screen:?}"
    );
    assert!(card.iter().any(|t| t.contains("AT €5.20/GB")), "{card:?}");
    // Chrome's 2.4 GB at €5.20 is €12; at $14.34 it was $34.
    assert!(screen.iter().any(|t| t == "€12"), "{screen:?}");
    // An empty price goes back to the file's.
    live.input(Input::Key(Key::Enter), now);
    let commands = type_text(&mut live, "", now);
    assert!(matches!(&commands[..], [Command::ApplySettings(s)] if s.custom_price.is_none()));
    assert_eq!(live.building().unwrap().price.symbol, "$");
}

#[test]
fn a_price_that_cannot_be_read_changes_nothing() {
    let (mut live, now) = live(Some(MemoryType::Ddr5), Settings::default());
    live.input(Input::Key(Key::F3), now);
    for _ in 0..5 {
        live.input(Input::Key(Key::Down), now);
    }
    live.input(Input::Key(Key::Enter), now);
    assert!(type_text(&mut live, "abc", now).is_empty());
    assert_eq!(live.settings().custom_price, None);
    // Esc while typing undoes the typing; Esc again leaves the screen.
    live.input(Input::Key(Key::Enter), now);
    live.input(Input::Char('7'), now);
    live.input(Input::Key(Key::Backspace), now);
    live.input(Input::Char('9'), now);
    live.input(Input::Key(Key::Escape), now);
    assert_eq!(live.ui().editing, None);
    assert_eq!(live.settings().custom_price, None);
    live.input(Input::Key(Key::Escape), now);
    assert_eq!(live.ui().view, View::Roll);
}

#[test]
fn estimates_are_marked_and_an_unknown_type_is_asked_for() {
    let (live_lp, _) = live(Some(MemoryType::Lpddr5), Settings::default());
    let b = live_lp.building().unwrap();
    assert!(b.price.estimated);
    let (screen, card) = screen_and_card(b, &Panel::Roll, false);
    assert!(
        screen.iter().any(|t| t.starts_with("LPDDR5 ~$14.34/GB")),
        "{screen:?}"
    );
    assert!(
        card.iter().any(|t| t.ends_with("$14.34/GB EST.")),
        "{card:?}"
    );

    let (mut unknown, _) = live(None, Settings::default());
    let b = unknown.building().unwrap();
    assert_eq!(b.memory_type, "RAM");
    assert_eq!(b.price.per_gb_micros, 6_530_000);
    assert_eq!(b.remark, "What RAM is this? Pick its type in settings: F3.");
    // Choosing a type in settings bills it at once.
    let now = snapshot("snapshot-desk.json").at_ms;
    unknown.input(Input::Key(Key::F3), now);
    for _ in 0..4 {
        unknown.input(Input::Key(Key::Down), now);
    }
    for _ in 0..4 {
        unknown.input(Input::Key(Key::Enter), now); // DDR3, DDR4, LPDDR4, DDR5
    }
    assert_eq!(unknown.settings().memory_type, Some(MemoryType::Ddr5));
    let b = unknown.building().unwrap();
    assert_eq!((b.memory_type.as_str(), b.price.estimated), ("DDR5", false));
}

#[test]
fn the_settings_screen_lists_rows_and_clicks_change_them() {
    let (mut live, now) = live(Some(MemoryType::Ddr4), Settings::default());
    live.input(Input::Key(Key::F3), now);
    let row = |live: &Live, k: u8| {
        live.regions()
            .iter()
            .find(|r| r.target == ramlord_core::ui::Target::Setting(k))
            .copied()
            .unwrap()
    };
    // A click on SCALE steps it to 2X.
    let r = row(&live, 0);
    let commands = live.input(
        Input::Click {
            x: r.x0 + 2,
            y: r.y0 + 2,
        },
        now,
    );
    assert!(matches!(&commands[..], [Command::ApplySettings(s)] if s.scale == Some(2)));
    // ANONYMOUS F2 on: the card names its tenants TENANT 1, 2...
    let r = row(&live, 9);
    live.input(
        Input::Click {
            x: r.x0 + 2,
            y: r.y0 + 2,
        },
        now,
    );
    assert!(live.settings().anonymous);
    let b = live.building().unwrap().clone();
    let (_, card) = screen_and_card(&b, &Panel::Roll, true);
    assert!(card.iter().any(|t| t == "TENANT 1"));
    assert!(!card.iter().any(|t| t == "CHROME"));
    assert!(live.card().is_some());
    // F2 asks the Windows layer for the card, from any view.
    assert_eq!(
        live.input(Input::Key(Key::F2), now),
        [Command::RentRollCard]
    );
    // A click on the tower still selects a tenant.
    live.input(Input::Key(Key::Escape), now);
    assert_eq!(live.ui().view, View::Roll);
}

#[test]
fn the_card_lists_ten_tenants_even_when_the_panel_lists_eight() {
    let s = snapshot("snapshot-gallery.json");
    let b = ramlord_core::tenants::building(&s, &Rules::builtin(), &Lease::placeholder());
    let (layout, _) = Layout::new(&b, &HueBook::new());
    assert_eq!(scene::Roll::new(&b, &layout).rows.len(), 8);
    let card = texts(&scene::card_with(&b, &layout, false));
    assert!(card.iter().any(|t| t == "OTHERS (2)"), "{card:?}");
}

#[test]
fn the_tray_icon_fills_with_occupancy_and_turns_red_when_full() {
    let count = |occupancy, full, color| {
        render_tray_icon(occupancy, full)
            .pixels
            .iter()
            .filter(|&&p| p == color)
            .count()
    };
    assert_eq!(count(0, false, GOLD), 0);
    assert_eq!(count(50, false, GOLD), 6 * 4);
    assert_eq!(count(100, false, GOLD), 12 * 4);
    assert_eq!(count(50, false, TRAY_FULL), 0);
    assert!(count(95, true, TRAY_FULL) > 40);
    // The corners are clear, so the taskbar shows through.
    save(&render_tray_icon(30, false), 8, "tray-30.png");
    save(&render_tray_icon(95, true), 8, "tray-full.png");
    let icon = render_tray_icon(50, false);
    assert_eq!(icon.pixel(0, 15), TRAY_CLEAR);
    assert_eq!((icon.width, icon.height), (16, 16));
}

/// A settings file with a price the building cannot be billed at (a
/// hand edit, a half-written file from another version) must not stop
/// the first frame: the price is dropped and the file's price bills.
#[test]
fn a_broken_price_in_the_settings_file_does_not_stop_the_app() {
    use ramlord_core::settings::CustomPrice;
    for (micros, as_of) in [(0, "2026-11-10"), (5_000_000, "not a date")] {
        let settings = Settings {
            custom_price: Some(CustomPrice {
                per_gb_micros: micros,
                symbol: "$".into(),
                as_of: as_of.into(),
            }),
            scale: Some(77),
            away_hours: 0,
            ..Settings::default()
        };
        let (live, _) = live(Some(MemoryType::Ddr4), settings);
        let b = live.building().unwrap();
        b.validate().expect("billable");
        assert_eq!(b.price.per_gb_micros, 6_530_000, "the file's DDR4 price");
        assert_eq!(live.settings().custom_price, None);
        assert_eq!(live.settings().scale, None);
        assert_eq!(live.settings().away_hours, 1);
    }
}
