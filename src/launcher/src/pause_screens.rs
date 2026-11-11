//! The in-game menu's screens: the menu itself and its pages for controls, core options,
//! cheats and achievements, each drawn over the dimmed game.

use crate::achievements::{self, Badges, GameNotices, Line, Page as AchievementsPage, Status, Summary};
use crate::canvas::{text_width, Canvas, Theme, BODY, MUTED, TEXT};
use crate::cheats::CheatsPage;
use crate::color::Rgb;
use crate::controls::{self, ControlsMenu, Row};
use crate::core_options::OptionsPage;
use crate::font;
use crate::i18n::{tr, trf};
use crate::pause::{Item, List, PauseMenu, MOST_ROWS};
use crate::screens::{self, OptionsLayout};
use crate::settings::{Border, GameDisplay, SwitcherStyle};
use crate::switcher::{SwitchPage, LARGE_COVER, SMALL_COVER};
use crate::systems::SYSTEMS;
use crate::{icons, image::Image};

/// What the in-game menu shows besides its choices.
pub struct PauseView<'a> {
    pub menu: &'a PauseMenu,
    /// Whether the selected slot holds a state.
    pub slot_used: bool,
    pub brightness_percent: Option<u32>,
    pub display: GameDisplay,
    /// What AUTO gives this game's console.
    pub auto_border: Border,
    /// Where the game shows each kind of achievement notice, or Settings' choice.
    pub notices: GameNotices,
}

const PAUSE_PANEL: (usize, usize, usize, usize) = (110, 15, 420, 450);
const PAUSE_TOP: usize = 75;
/// Roomy enough for the eye to find a row, with the longest list still fitting.
const PAUSE_ROW: usize = 32;
const PAUSE_TEXT_SCALE: usize = 2;
const PAUSE_BAR_WIDTH: usize = 120;

// The rows, message and hint must fit inside the panel.
const _: () = assert!(PAUSE_TOP + MOST_ROWS * PAUSE_ROW + 70 <= PAUSE_PANEL.1 + PAUSE_PANEL.3);

/// Darkens the game frame, so the menu stands out while the game stays recognisable.
fn dim(pixel: u32) -> Rgb {
    Rgb((pixel >> 16) as u8 / 3, (pixel >> 8) as u8 / 3, pixel as u8 / 3)
}

/// The paused game's last frame, dimmed, with the panel the in-game menu's pages go on.
fn pause_backdrop(width: usize, height: usize, frame: &[u32], frame_size: (usize, usize), theme: Theme) -> Canvas {
    backdrop_with_panel(width, height, frame, frame_size, theme, PAUSE_PANEL)
}

fn backdrop_with_panel(
    width: usize,
    height: usize,
    frame: &[u32],
    (frame_w, frame_h): (usize, usize),
    theme: Theme,
    (panel_x, panel_y, panel_w, panel_h): (usize, usize, usize, usize),
) -> Canvas {
    let mut canvas = Canvas::new(width, height, theme);
    canvas.pixels.fill(Rgb(0, 0, 0));
    let left = width.saturating_sub(frame_w) / 2;
    let top = height.saturating_sub(frame_h) / 2;
    if frame_w > 0 && frame.len() >= frame_w * frame_h {
        for (y, row) in frame.chunks_exact(frame_w).take(height - top).enumerate() {
            for (x, &pixel) in row.iter().take(width - left).enumerate() {
                canvas.pixels[(top + y) * width + left + x] = dim(pixel);
            }
        }
    }

    canvas.rect(panel_x, panel_y, panel_w, panel_h, canvas.theme.background);
    canvas
}

/// Lays out the in-game menu over the paused game's last frame.
pub fn pause_screen(
    width: usize,
    height: usize,
    frame: &[u32],
    frame_size: (usize, usize),
    view: &PauseView,
    theme: Theme,
) -> Canvas {
    let mut canvas = pause_backdrop(width, height, frame, frame_size, theme);
    let (panel_x, panel_y, panel_w, _) = PAUSE_PANEL;
    let menu = view.menu;
    canvas.text(menu.list().title(), panel_y + 20, 3, TEXT);

    let label_x = panel_x + 20;
    let value_right = panel_x + panel_w - 20;
    for (row, &item) in menu.items().iter().enumerate() {
        let y = PAUSE_TOP + row * PAUSE_ROW;
        let selected = item == menu.selected();
        let (text_color, bar_color) = if selected {
            canvas.rect(panel_x, y, panel_w, PAUSE_ROW - 4, canvas.theme.accent);
            (canvas.theme.background, canvas.theme.background)
        } else {
            (TEXT, canvas.theme.accent)
        };
        let text_y = y + (PAUSE_ROW - 4 - font::HEIGHT * PAUSE_TEXT_SCALE) / 2;
        canvas.text_at(item.label(), label_x, text_y, PAUSE_TEXT_SCALE, text_color);

        if item == Item::Brightness {
            let level = view.brightness_percent;
            let bar_x = value_right - PAUSE_BAR_WIDTH;
            let bar_y = y + (PAUSE_ROW - 4) / 2 - 6;
            canvas.rect(bar_x, bar_y, PAUSE_BAR_WIDTH, 12, bar_color);
            canvas.rect(bar_x + 2, bar_y + 2, PAUSE_BAR_WIDTH - 4, 8, if selected { canvas.theme.accent } else { canvas.theme.background });
            let fill = (PAUSE_BAR_WIDTH - 4) * level.unwrap_or(0).min(100) as usize / 100;
            canvas.rect(bar_x + 2, bar_y + 2, fill, 8, bar_color);
            continue;
        }
        let value = match item {
            Item::Scaling => view.display.scaling.label().to_string(),
            Item::Effect => view.display.effect.label().to_string(),
            Item::Ghosting => view.display.ghosting.label().to_string(),
            Item::Border if view.display.border == Border::Auto => trf("AUTO ({})", &[&view.auto_border.label()]),
            Item::Border => view.display.border.label().to_string(),
            Item::SaveState => trf("SLOT {}", &[&menu.slot()]),
            Item::LoadState if view.slot_used => trf("SLOT {}", &[&menu.slot()]),
            Item::LoadState => trf("SLOT {} EMPTY", &[&menu.slot()]),
            Item::Disc => trf("DISC {} OF {}", &[&(menu.disc() + 1), &menu.discs()]),
            Item::Quit => match menu.start_over() {
                Some(true) => tr("START OVER").to_string(),
                Some(false) => tr("KEEP PLACE").to_string(),
                None => continue,
            },
            Item::UnlockNotices | Item::ProgressNotices | Item::OtherNotices => {
                let Some(kind) = item.notice_kind() else { continue };
                // Settings' choice in full is too wide for the German labels.
                view.notices.get(kind).map_or(tr("DEFAULT"), |place| place.label()).to_string()
            }
            _ if item.opens() => ">".to_string(),
            _ => continue,
        };
        let value = if selected && !item.opens() { format!("< {value} >") } else { value };
        let value_x = value_right - text_width(&value, PAUSE_TEXT_SCALE);
        canvas.text_at(&value, value_x, text_y, PAUSE_TEXT_SCALE, text_color);
    }

    // Below the longest list, so the hint stays put from list to list.
    let message_y = PAUSE_TOP + MOST_ROWS * PAUSE_ROW + 12;
    if let Some(message) = menu.message() {
        canvas.text(message, message_y, PAUSE_TEXT_SCALE, canvas.theme.accent);
    }
    let hint = if menu.list() == List::Main { tr("A SELECTS - B RESUMES") } else { tr("A SELECTS - B BACK") };
    canvas.text(hint, message_y + 30, PAUSE_TEXT_SCALE, MUTED);
    canvas
}

/// As much of `text` as fits in `width` pixels at the menu's text size.
fn fitting(text: &str, width: usize) -> String {
    let columns = width / ((font::WIDTH + 1) * PAUSE_TEXT_SCALE);
    text.chars().take(columns).collect()
}

/// The in-game menu's SWITCH GAME page, in the style Settings > THEME chose.
pub fn switch_screen(width: usize, height: usize, frame: &[u32], frame_size: (usize, usize), page: &SwitchPage, theme: Theme) -> Canvas {
    match page.style() {
        SwitcherStyle::List => switch_list(width, height, frame, frame_size, page, theme),
        SwitcherStyle::Covers => switch_covers(width, height, frame, frame_size, page, theme),
    }
}

/// The other games as rows, like the menu's lists, each with its console on the right.
fn switch_list(width: usize, height: usize, frame: &[u32], frame_size: (usize, usize), page: &SwitchPage, theme: Theme) -> Canvas {
    let mut canvas = pause_backdrop(width, height, frame, frame_size, theme);
    let (panel_x, panel_y, panel_w, _) = PAUSE_PANEL;
    canvas.text(tr("SWITCH GAME"), panel_y + 20, 3, TEXT);
    let label_x = panel_x + 20;
    let value_right = panel_x + panel_w - 20;
    for (row, game) in page.games().iter().take(MOST_ROWS).enumerate() {
        let y = PAUSE_TOP + row * PAUSE_ROW;
        let selected = row == page.selected_index();
        let (text_color, console_color) = if selected {
            canvas.rect(panel_x, y, panel_w, PAUSE_ROW - 4, canvas.theme.accent);
            (canvas.theme.background, canvas.theme.background)
        } else {
            (TEXT, MUTED)
        };
        let text_y = y + (PAUSE_ROW - 4 - font::HEIGHT * PAUSE_TEXT_SCALE) / 2;
        let console = SYSTEMS[game.system].label;
        let console_x = value_right - text_width(console, PAUSE_TEXT_SCALE);
        canvas.text_at(console, console_x, text_y, PAUSE_TEXT_SCALE, console_color);
        let name = fitting(&game.name, console_x.saturating_sub(label_x + 12));
        canvas.text_at(&name, label_x, text_y, PAUSE_TEXT_SCALE, text_color);
    }
    let message_y = PAUSE_TOP + MOST_ROWS * PAUSE_ROW + 12;
    let hint = if page.resumes() { tr("A SWITCHES - B RESUMES") } else { tr("A SWITCHES - B BACK") };
    canvas.text(hint, message_y + 30, PAUSE_TEXT_SCALE, MUTED);
    canvas
}

/// Where the selected cover's area starts, and how far apart the covers' middles are.
const SWITCH_COVER_TOP: usize = 84;
const SWITCH_NEAR: usize = LARGE_COVER.0 / 2 + 12 + SMALL_COVER.0 / 2;
const SWITCH_FAR: usize = SWITCH_NEAR + SMALL_COVER.0 + 10;
const SWITCH_NAME_Y: usize = SWITCH_COVER_TOP + LARGE_COVER.1 + 20;
const SWITCH_HINT_Y: usize = OPTIONS_PANEL.1 + OPTIONS_PANEL.3 - 34;
// The outer covers stay inside the panel, and the console's line above the hint.
const _: () = assert!(320 - SWITCH_FAR - SMALL_COVER.0 / 2 >= OPTIONS_PANEL.0);
const _: () = assert!(SWITCH_NAME_Y + 30 + font::HEIGHT * PAUSE_TEXT_SCALE + 8 <= SWITCH_HINT_Y);

/// The covers side by side, the selected one large in the middle, with its name below.
fn switch_covers(width: usize, height: usize, frame: &[u32], frame_size: (usize, usize), page: &SwitchPage, theme: Theme) -> Canvas {
    let mut canvas = backdrop_with_panel(width, height, frame, frame_size, theme, OPTIONS_PANEL);
    canvas.text(tr("SWITCH GAME"), OPTIONS_TITLE_Y, 3, TEXT);
    let middle = (width / 2, SWITCH_COVER_TOP + LARGE_COVER.1 / 2);
    let selected = page.selected_index();
    for (offset, place) in [(-2, -(SWITCH_FAR as isize)), (-1, -(SWITCH_NEAR as isize)), (1, SWITCH_NEAR as isize), (2, SWITCH_FAR as isize)] {
        let Some(index) = selected.checked_add_signed(offset).filter(|&index| index < page.games().len()) else { continue };
        let x = middle.0.saturating_add_signed(place);
        draw_switch_cover(&mut canvas, page, index, (x, middle.1), false);
    }
    draw_switch_cover(&mut canvas, page, selected, middle, true);

    let game = page.selected();
    let name = fitting(&game.name, OPTIONS_PANEL.2 - 32);
    canvas.text(&name, SWITCH_NAME_Y, PAUSE_TEXT_SCALE, TEXT);
    let place = format!("{} - {} / {}", SYSTEMS[game.system].label, selected + 1, page.games().len());
    canvas.text(&place, SWITCH_NAME_Y + 30, PAUSE_TEXT_SCALE, MUTED);
    let hint = if page.resumes() {
        tr("A SWITCHES - LEFT RIGHT CHOOSES - B RESUMES")
    } else {
        tr("A SWITCHES - LEFT RIGHT CHOOSES - B BACK")
    };
    canvas.text(hint, SWITCH_HINT_Y, PAUSE_TEXT_SCALE, MUTED);
    canvas
}

/// One game's cover centred on `centre`, or its console's pictogram on a plain card. The
/// selected one is large, in a frame of the theme's colour.
fn draw_switch_cover(canvas: &mut Canvas, page: &SwitchPage, index: usize, (centre_x, centre_y): (usize, usize), selected: bool) {
    let image: Option<&Image> = page.cover(index).map(|cover| if selected { &cover.large } else { &cover.small });
    let (box_w, box_h) = if selected { LARGE_COVER } else { SMALL_COVER };
    let (w, h) = image.map_or((box_w, box_h * 3 / 4), |image| (image.width, image.height));
    let (x, y) = (centre_x.saturating_sub(w / 2), centre_y.saturating_sub(h / 2));
    if selected {
        canvas.rect(x.saturating_sub(4), y.saturating_sub(4), w + 8, h + 8, canvas.theme.accent);
    }
    match image {
        Some(image) => canvas.image(image, x, y),
        None => {
            canvas.rect(x, y, w, h, BODY);
            let scale = if selected { 4 } else { 2 };
            let icon = SYSTEMS[page.games()[index].system].icon;
            canvas.icon(icon, centre_x.saturating_sub(icons::WIDTH * scale / 2), centre_y.saturating_sub(icons::HEIGHT * scale / 2), scale);
        }
    }
}

const CONTROLS_TOP: usize = 60;
/// Tighter than the menu's, for a row each of the buttons and a heading over them.
const CONTROLS_ROW: usize = 24;

// The rows, the heading, the note and the hint, whose text ends 56 lines below the rows.
const _: () = assert!(CONTROLS_TOP + (controls::ROW_COUNT + 1) * CONTROLS_ROW + 56 <= PAUSE_PANEL.1 + PAUSE_PANEL.3);

/// The in-game menu's CONTROLS page: over the same panel, a row for each button.
pub fn controls_screen(
    width: usize,
    height: usize,
    frame: &[u32],
    frame_size: (usize, usize),
    menu: &ControlsMenu,
    theme: Theme,
) -> Canvas {
    let mut canvas = pause_backdrop(width, height, frame, frame_size, theme);
    let (panel_x, panel_y, panel_w, _) = PAUSE_PANEL;
    canvas.text(tr("CONTROLS"), panel_y + 20, 3, TEXT);

    let label_x = panel_x + 20;
    let value_right = panel_x + panel_w - 20;
    let text_y = |y: usize| y + (CONTROLS_ROW - 2 - font::HEIGHT * PAUSE_TEXT_SCALE) / 2;
    let mut y = CONTROLS_TOP;
    for row in controls::rows() {
        if matches!(row, Row::Button(0)) {
            // Which side of a row is which isn't obvious from two button names.
            canvas.text_at(tr("BUTTON"), label_x, text_y(y), PAUSE_TEXT_SCALE, MUTED);
            let heading = tr("IN THE GAME");
            canvas.text_at(heading, value_right - text_width(heading, PAUSE_TEXT_SCALE), text_y(y), PAUSE_TEXT_SCALE, MUTED);
            y += CONTROLS_ROW;
        }
        let selected = row == menu.selected();
        let color = if selected {
            canvas.rect(panel_x, y, panel_w, CONTROLS_ROW - 2, canvas.theme.accent);
            canvas.theme.background
        } else {
            TEXT
        };
        canvas.text_at(ControlsMenu::label(row), label_x, text_y(y), PAUSE_TEXT_SCALE, color);
        if let Some(value) = menu.value(row) {
            let value = if selected { format!("< {value} >") } else { value };
            canvas.text_at(&value, value_right - text_width(&value, PAUSE_TEXT_SCALE), text_y(y), PAUSE_TEXT_SCALE, color);
        }
        y += CONTROLS_ROW;
    }
    if let Some(note) = menu.note() {
        canvas.text(note, y + 12, PAUSE_TEXT_SCALE, canvas.theme.accent);
    }
    canvas.text(tr("< > CHANGES - A TURBO - B BACK"), y + 42, PAUSE_TEXT_SCALE, MUTED);
    canvas
}

/// Wider than the menu's, as option names and choices come from the cores, at any length.
const OPTIONS_PANEL: (usize, usize, usize, usize) = (20, 15, 600, 450);
const OPTIONS_ROWS: OptionsLayout =
    OptionsLayout { left: OPTIONS_PANEL.0, right: OPTIONS_PANEL.0 + OPTIONS_PANEL.2, pad: 16, top: 60, row: 24, shown: 12 };

// The rows, the note, then a line about restarting and the hint, 50 lines below the note.
const _: () = assert!(OPTIONS_ROWS.top + screens::option_rows_height(&OPTIONS_ROWS) + 50 <= OPTIONS_PANEL.1 + OPTIONS_PANEL.3);
/// Higher than the menu's title, so the arrow for rows above the page, 10 lines over the
/// rows, stays clear of it.
const OPTIONS_TITLE_Y: usize = OPTIONS_PANEL.1 + 12;
const _: () = assert!(OPTIONS_TITLE_Y + font::HEIGHT * 3 < OPTIONS_ROWS.top - 10);

/// The in-game menu's CORE OPTIONS page.
pub fn core_options_screen(
    width: usize,
    height: usize,
    frame: &[u32],
    frame_size: (usize, usize),
    page: &OptionsPage,
    theme: Theme,
) -> Canvas {
    let mut canvas = backdrop_with_panel(width, height, frame, frame_size, theme, OPTIONS_PANEL);
    let title: String = page.category_label().unwrap_or(tr("CORE OPTIONS")).chars().take(28).collect();
    canvas.text(&title, OPTIONS_TITLE_Y, 3, TEXT);
    screens::option_rows(&mut canvas, page, &OPTIONS_ROWS);
    let below = OPTIONS_ROWS.top + screens::option_rows_height(&OPTIONS_ROWS);
    canvas.text(tr("SOME CHANGES APPLY ON THE GAME'S NEXT START"), below + 6, PAUSE_TEXT_SCALE, MUTED);
    canvas.text(tr("< > CHANGES - A OPENS - B BACK"), below + 30, PAUSE_TEXT_SCALE, MUTED);
    canvas
}

/// The in-game menu's CHEATS page, on the same wide panel as CORE OPTIONS, as codes'
/// descriptions come from the files.
pub fn cheats_screen(
    width: usize,
    height: usize,
    frame: &[u32],
    frame_size: (usize, usize),
    page: &CheatsPage,
    theme: Theme,
) -> Canvas {
    let mut canvas = backdrop_with_panel(width, height, frame, frame_size, theme, OPTIONS_PANEL);
    canvas.text(tr("CHEATS"), OPTIONS_TITLE_Y, 3, TEXT);
    screens::option_rows(&mut canvas, page, &OPTIONS_ROWS);
    let below = OPTIONS_ROWS.top + screens::option_rows_height(&OPTIONS_ROWS);
    match page.message() {
        Some(message) => canvas.text(message, below + 6, PAUSE_TEXT_SCALE, canvas.theme.accent),
        None if page.has_cheats() => canvas.text(tr("CHEATS CAN CHANGE OR SPOIL SAVES"), below + 6, PAUSE_TEXT_SCALE, MUTED),
        None => {}
    }
    let hint = if page.has_cheats() { tr("A TURNS ON OR OFF - L1 R1 PAGES - B BACK") } else { tr("A SELECTS - B BACK") };
    canvas.text(hint, below + 30, PAUSE_TEXT_SCALE, MUTED);
    canvas
}

/// What the in-game menu's ACHIEVEMENTS page shows.
pub struct AchievementsView<'a> {
    pub page: &'a AchievementsPage,
    pub status: &'a Status,
    pub summary: Summary,
}

/// Lines on the ACHIEVEMENTS page at once, which L1 and R1 page by.
pub const ACHIEVEMENT_ROWS: usize = 8;
const ACHIEVEMENT_TOP: usize = 84;
const ACHIEVEMENT_ROW: usize = 36;
const ACHIEVEMENT_SUMMARY_Y: usize = OPTIONS_TITLE_Y + font::HEIGHT * 3 + 12;
/// Below the rows, and the line that parts them from the description.
const ACHIEVEMENT_RULE_Y: usize = ACHIEVEMENT_TOP + ACHIEVEMENT_ROWS * ACHIEVEMENT_ROW + 4;
const ACHIEVEMENT_DESCRIPTION_Y: usize = ACHIEVEMENT_RULE_Y + 12;
const ACHIEVEMENT_HINT_Y: usize = ACHIEVEMENT_DESCRIPTION_Y + 2 * 20 + 6;
// The summary clears the title, and the hint ends inside the panel.
const _: () = assert!(ACHIEVEMENT_SUMMARY_Y + font::HEIGHT * PAUSE_TEXT_SCALE + 8 <= ACHIEVEMENT_TOP);
const _: () = assert!(ACHIEVEMENT_HINT_Y + font::HEIGHT * PAUSE_TEXT_SCALE <= OPTIONS_PANEL.1 + OPTIONS_PANEL.3);


/// The in-game menu's ACHIEVEMENTS page, on the wide panel: the game's achievements with
/// their badges, grey while locked, and the selected one's description below them.
pub fn achievements_screen(
    width: usize,
    height: usize,
    frame: &[u32],
    frame_size: (usize, usize),
    view: &AchievementsView,
    badges: &mut Badges,
    theme: Theme,
) -> Canvas {
    let mut canvas = backdrop_with_panel(width, height, frame, frame_size, theme, OPTIONS_PANEL);
    let (panel_x, _, panel_w, _) = OPTIONS_PANEL;
    canvas.text(tr("ACHIEVEMENTS"), OPTIONS_TITLE_Y, 3, TEXT);
    let waiting = match view.status {
        Status::Loading => Some(tr("LOADING THE ACHIEVEMENTS").to_string()),
        Status::NoAchievements => Some(tr("NO ACHIEVEMENTS FOR THIS GAME YET").to_string()),
        Status::NotRecognised => Some(tr("RETROACHIEVEMENTS DOESN'T KNOW THIS GAME").to_string()),
        Status::Failed(message) => Some(message.clone()),
        Status::Ready => None,
    };
    if let Some(message) = waiting {
        canvas.text(&message, 220, PAUSE_TEXT_SCALE, MUTED);
        canvas.text(tr("B BACK"), ACHIEVEMENT_HINT_Y, PAUSE_TEXT_SCALE, MUTED);
        return canvas;
    }
    let summary = view.summary;
    let mut line = trf("{} OF {} UNLOCKED - {}/{} POINTS", &[&summary.unlocked, &summary.total, &summary.points, &summary.points_total]);
    if summary.hardcore {
        line = format!("{line} - {}", tr("HARDCORE"));
    }
    canvas.text(&line, ACHIEVEMENT_SUMMARY_Y, PAUSE_TEXT_SCALE, MUTED);

    let advance = (font::WIDTH + 1) * PAUSE_TEXT_SCALE;
    let (left, right) = (panel_x + 16, panel_x + panel_w - 16);
    let page = view.page;
    let first = page.first_shown(ACHIEVEMENT_ROWS);
    for (row, (index, line)) in page.lines().iter().enumerate().skip(first).take(ACHIEVEMENT_ROWS).enumerate() {
        let y = ACHIEVEMENT_TOP + row * ACHIEVEMENT_ROW;
        let text_y = y + (ACHIEVEMENT_ROW - 2 - font::HEIGHT * PAUSE_TEXT_SCALE) / 2;
        let entry = match line {
            Line::Heading(label) => {
                canvas.text_at(label, left, text_y + 4, PAUSE_TEXT_SCALE, canvas.theme.accent);
                continue;
            }
            Line::Achievement(entry) => entry,
        };
        let selected = index == page.selected();
        let color = if selected {
            canvas.rect(panel_x, y, panel_w, ACHIEVEMENT_ROW - 2, canvas.theme.accent);
            canvas.theme.background
        } else if entry.unlocked {
            TEXT
        } else {
            MUTED
        };
        let badge_y = y + (ACHIEVEMENT_ROW - 2 - achievements::SMALL_BADGE) / 2;
        match badges.get(&entry.badge, achievements::SMALL_BADGE, !entry.unlocked) {
            Some(image) => canvas.image(&image, left, badge_y),
            None => canvas.rect(left, badge_y, achievements::SMALL_BADGE, achievements::SMALL_BADGE, BODY),
        }
        // Progress for one under way, or else its points.
        let value = if !entry.unlocked && !entry.progress.is_empty() { entry.progress.clone() } else { entry.points.to_string() };
        let value_x = right - text_width(&value, PAUSE_TEXT_SCALE);
        canvas.text_cased_at(&value, value_x, text_y, PAUSE_TEXT_SCALE, color);
        let title_x = left + achievements::SMALL_BADGE + 10;
        let columns = value_x.saturating_sub(title_x + 12) / advance;
        canvas.text_cased_at(&screens::cut(&entry.title, columns), title_x, text_y, PAUSE_TEXT_SCALE, color);
    }
    canvas.rect(left, ACHIEVEMENT_RULE_Y, right - left, 2, BODY);
    if let Some(entry) = page.selected_entry() {
        let columns = (right - left) / advance;
        for (number, text) in screens::wrapped_lines(&entry.description, columns, 2).iter().enumerate() {
            canvas.text_cased_at(text, left, ACHIEVEMENT_DESCRIPTION_Y + number * 20, PAUSE_TEXT_SCALE, TEXT);
        }
    }
    canvas.text(tr("L1 R1 PAGES - B BACK"), ACHIEVEMENT_HINT_Y, PAUSE_TEXT_SCALE, MUTED);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Button;

    fn pixel(canvas: &Canvas, x: usize, y: usize) -> Rgb {
        canvas.pixels[y * canvas.width + x]
    }

    fn pause_view(menu: &PauseMenu) -> PauseView<'_> {
        PauseView {
            menu,
            slot_used: false,
            brightness_percent: None,
            display: GameDisplay::default(),
            auto_border: Border::Bezel,
            notices: GameNotices::default(),
        }
    }

    #[test]
    fn the_pause_menu_dims_the_frame_and_highlights_the_selection() {
        let frame = vec![0x00FF_FFFFu32; 640 * 426];
        let menu = PauseMenu::new(1, true);
        let canvas = pause_screen(640, 480, &frame, (640, 426), &pause_view(&menu), Theme::default());
        // Outside the panel, the frame shows dimmed. The bars above and below are black.
        assert_eq!(pixel(&canvas, 5, 240), Rgb(85, 85, 85));
        assert_eq!(pixel(&canvas, 5, 5), Rgb(0, 0, 0));
        // Resume is selected first.
        assert_eq!(pixel(&canvas, PAUSE_PANEL.0 + 2, PAUSE_TOP + 2), Theme::default().accent);
        assert_eq!(pixel(&canvas, PAUSE_PANEL.0 + 2, PAUSE_TOP + PAUSE_ROW + 2), Theme::default().background);
    }

    #[test]
    fn the_controls_page_highlights_its_selection_and_keeps_every_row_on_the_panel() {
        let mut menu = ControlsMenu::new(crate::controls::Layout::default(), None);
        let canvas = controls_screen(640, 480, &[], (0, 0), &menu, Theme::default());
        assert_eq!(pixel(&canvas, PAUSE_PANEL.0 + 2, CONTROLS_TOP + 2), Theme::default().accent);
        // The heading stands between the first row and the second, which isn't selected.
        assert_eq!(pixel(&canvas, PAUSE_PANEL.0 + 2, CONTROLS_TOP + 2 * CONTROLS_ROW + 2), Theme::default().background);
        menu.press(Button::Down);
        let canvas = controls_screen(640, 480, &[], (0, 0), &menu, Theme::default());
        assert_eq!(pixel(&canvas, PAUSE_PANEL.0 + 2, CONTROLS_TOP + 2 * CONTROLS_ROW + 2), Theme::default().accent);
    }

    #[test]
    fn the_longest_control_fits_beside_its_label() {
        let value = text_width("< RECHTS TURBO >", PAUSE_TEXT_SCALE);
        let label = text_width("SELECT", PAUSE_TEXT_SCALE);
        assert!(20 + label + 12 + value + 20 <= PAUSE_PANEL.2);
    }

    #[test]
    fn the_pause_menu_survives_odd_sizes() {
        let menu = PauseMenu::new(1, true);
        let canvas = pause_screen(64, 48, &[0; 4], (2, 2), &pause_view(&menu), Theme::default());
        assert_eq!(canvas.pixels.len(), 64 * 48);
        let canvas = pause_screen(640, 480, &[], (0, 0), &pause_view(&menu), Theme::default());
        assert_eq!(canvas.pixels.len(), 640 * 480);
    }

    #[test]
    fn the_longest_slot_text_fits_beside_its_label() {
        let value = text_width(&format!("< SLOT {} EMPTY >", crate::pause::SLOTS), PAUSE_TEXT_SCALE);
        let label = text_width("LOAD STATE", PAUSE_TEXT_SCALE);
        assert!(20 + label + 12 + value + 20 <= PAUSE_PANEL.2);
    }

    #[test]
    fn the_longest_disc_text_fits_beside_its_label() {
        let value = text_width("< DISC 8 OF 8 >", PAUSE_TEXT_SCALE);
        let label = text_width("CHANGE DISC", PAUSE_TEXT_SCALE);
        assert!(20 + label + 12 + value + 20 <= PAUSE_PANEL.2);
    }
}
