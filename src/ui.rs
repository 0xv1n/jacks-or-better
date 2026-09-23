use std::f32::consts::PI;
use std::sync::{Arc, LazyLock};

use eframe::egui::layers::ShapeIdx;
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    Align, Align2, Button, CentralPanel, Color32, ComboBox, Context, CornerRadius, CursorIcon,
    Event, FontData, FontDefinitions, FontFamily, FontId, Frame, InputState, Key, Layout, Margin,
    Mesh, Painter, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind, Theme, Ui,
    UiBuilder, Vec2, WidgetInfo, WidgetType, pos2, vec2,
};
use eframe::epaint::{Shadow, Tessellator};
use serde::{Deserialize, Serialize};

use crate::cards::{Card, Suit};
use crate::game::{DENOMS, Feedback, Game, MAX_COINS, Phase, Saved};
use crate::hand::HandRank;
use crate::sound::Sound;

const BG_TOP: Color32 = Color32::from_rgb(16, 54, 46);
const BG_BOTTOM: Color32 = Color32::from_rgb(7, 12, 16);
const PANEL: Color32 = Color32::from_rgba_premultiplied(16, 16, 16, 16);
const PANEL_HOVER: Color32 = Color32::from_rgba_premultiplied(28, 28, 28, 28);
const POPUP: Color32 = Color32::from_rgb(20, 36, 34);
const PANEL_STROKE: Color32 = Color32::from_rgba_premultiplied(30, 30, 30, 30);
const GOLD: Color32 = Color32::from_rgb(242, 196, 78);
const COURT_GOLD: Color32 = Color32::from_rgb(206, 158, 52);
const TEXT: Color32 = Color32::from_rgb(234, 238, 242);
const MUTED: Color32 = Color32::from_rgb(140, 154, 166);
const DARK: Color32 = Color32::from_rgb(20, 22, 28);
const CARD: Color32 = Color32::from_rgb(250, 250, 247);
const CARD_BACK: Color32 = Color32::from_rgb(26, 50, 94);
const RED: Color32 = Color32::from_rgb(214, 48, 58);
const BLACK: Color32 = Color32::from_rgb(28, 30, 36);
const GREEN: Color32 = Color32::from_rgb(82, 205, 128);
const AMBER: Color32 = Color32::from_rgb(245, 166, 60);
const BAD: Color32 = Color32::from_rgb(240, 96, 96);

const SAVE_KEY: &str = "jacks";
const SETTINGS_KEY: &str = "settings";
const FLIP_SECS: f64 = 0.42;
/// Delay between neighboring cards starting to flip.
const FLIP_STAGGER: f64 = 0.06;
const COUNT_SECS: f64 = 0.8;
const POP_SECS: f64 = 0.35;
const PULSE_SECS: f64 = 1.2;
const CONTENT_MAX_W: f32 = 1040.0;
/// Window size (in points) the layout is designed for; larger windows zoom in.
const DESIGN_SIZE: Vec2 = vec2(1000.0, 820.0);
/// Room above the cards for a hover lift and the top of the HELD pill.
const TAG_H: f32 = 18.0;
const CARD_GAP: f32 = 16.0;
const CARD_MAX_W: f32 = 156.0;
const CARD_MIN_H: f32 = 120.0;
const TABLE_ROW_H: f32 = 26.0;
const TABLE_PAD: f32 = 12.0;
const TABLE_H: f32 = TABLE_ROW_H * 5.0 + TABLE_PAD * 2.0;
const FEEDBACK_H: f32 = 64.0;
const MINI_W: f32 = 34.0;

/// Built once: `FontFamily::Name` holds an `Arc<str>`, and this is called dozens of times per frame.
static BOLD_FAMILY: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("bold".into()));

fn bold(size: f32) -> FontId {
    FontId::new(size, BOLD_FAMILY.clone())
}

fn reg(size: f32) -> FontId {
    FontId::proportional(size)
}

fn denom_label(cents: u32) -> String {
    if cents >= 100 {
        format!("${}", cents / 100)
    } else {
        format!("{cents}¢")
    }
}

fn dollars(cents: u64) -> String {
    format!("${}.{:02}", cents / 100, cents % 100)
}

fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "inter-bold".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Bold.ttf"
        ))),
    );
    let prop = fonts.families.entry(FontFamily::Proportional).or_default();
    prop.insert(0, "inter".into());
    // Bold family keeps the default fonts as fallbacks for symbols Inter lacks.
    let mut bold_family = vec!["inter-bold".to_owned()];
    bold_family.extend(prop.iter().skip(1).cloned());
    fonts.families.insert(BOLD_FAMILY.clone(), bold_family);
    ctx.set_fonts(fonts);
}

/// A fresh key press, ignoring OS auto-repeat so holding Space cannot auto-play hands.
fn pressed_once(input: &InputState, key: Key) -> bool {
    input.events.iter().any(|e| {
        matches!(
            e,
            Event::Key {
                key: k,
                pressed: true,
                repeat: false,
                ..
            } if *k == key
        )
    })
}

/// Button actions that replace cards. They are applied at the start of the next frame,
/// before any card is painted, so the flip animation starts from the old face.
#[derive(Clone, Copy)]
enum Action {
    Primary,
    BetMax,
}

#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
enum Speed {
    #[default]
    Normal,
    Fast,
    Instant,
}

impl Speed {
    const ALL: [Speed; 3] = [Speed::Normal, Speed::Fast, Speed::Instant];

    fn label(self) -> &'static str {
        match self {
            Speed::Normal => "Normal",
            Speed::Fast => "Fast",
            Speed::Instant => "Instant",
        }
    }

    /// Multiplier for every animation duration.
    fn scale(self) -> f64 {
        match self {
            Speed::Normal => 1.0,
            Speed::Fast => 0.5,
            Speed::Instant => 0.0,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    muted: bool,
    speed: Speed,
}

/// Progress through an animation of length `secs`; instant animations are done at once.
fn progress(elapsed: f64, secs: f64) -> f32 {
    if secs <= 0.0 {
        1.0
    } else {
        (elapsed / secs).clamp(0.0, 1.0) as f32
    }
}

/// Shapes painted into a reserved slot at the end of the frame, scaled about `center`
/// and rotated in perspective by `angle` about the vertical axis through it.
struct Warp {
    slot: ShapeIdx,
    shapes: Vec<Shape>,
    center: Pos2,
    angle: f32,
    depth: f32,
    scale: f32,
}

pub struct App {
    game: Game,
    settings: Settings,
    sound: Sound,
    seen_gen: [u32; 5],
    seen_phase: Phase,
    flip_start: [f64; 5],
    /// Face drawn last frame per slot; becomes `prev_face` when the slot gets a new card.
    shown: [Option<Card>; 5],
    /// Face to show on the first half of a flip, before the new card is revealed.
    prev_face: [Option<Card>; 5],
    /// When the last drawn hand is fully face up; the result stays hidden until then.
    result_time: f64,
    queued: Option<Action>,
    /// Measured height of everything except the cards and the spare space around them.
    fixed_h: f32,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_theme(Theme::Dark);
        cc.egui_ctx.all_styles_mut(|s| {
            s.spacing.item_spacing = vec2(10.0, 8.0);
            // Popups and selections in the app's palette instead of egui's blue-gray.
            s.visuals.selection.bg_fill = GOLD;
            s.visuals.selection.stroke.color = DARK;
            s.visuals.window_fill = POPUP;
            s.visuals.window_stroke = Stroke::new(1.0, PANEL_STROKE);
            s.visuals.window_corner_radius = CornerRadius::same(10);
        });
        // The zoom follows the window size; don't let Ctrl +/- fight it.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let game = cc
            .storage
            .and_then(|s| eframe::get_value::<Saved>(s, SAVE_KEY))
            .map_or_else(Game::new, Game::restore);
        let settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, SETTINGS_KEY))
            .unwrap_or_default();
        App {
            game,
            settings,
            sound: Sound::new(),
            seen_gen: [0; 5],
            seen_phase: Phase::Ready,
            flip_start: [f64::NEG_INFINITY; 5],
            shown: [None; 5],
            prev_face: [None; 5],
            result_time: f64::NEG_INFINITY,
            queued: None,
            fixed_h: 480.0,
        }
    }

    fn flip_secs(&self) -> f64 {
        FLIP_SECS * self.settings.speed.scale()
    }

    fn primary_action(&mut self) {
        if self.game.is_bust() {
            self.game.reset();
            return;
        }
        match self.game.phase {
            Phase::Dealt => self.game.draw(),
            Phase::Ready => self.game.deal(),
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Primary => self.primary_action(),
            Action::BetMax => self.game.bet_max(),
        }
    }

    fn toggle_hold(&mut self, i: usize) {
        if self.game.phase == Phase::Dealt {
            self.game.toggle_hold(i);
            if !self.settings.muted {
                self.sound.hold(self.game.held[i]);
            }
        }
    }

    fn handle_keys(&mut self, ctx: &Context) {
        let (space, b, m, nums) = ctx.input_mut(|i| {
            let keys = (
                pressed_once(i, Key::Space),
                pressed_once(i, Key::B),
                pressed_once(i, Key::M),
                [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5].map(|k| pressed_once(i, k)),
            );
            // Space is global; don't let it also click a keyboard-focused button.
            i.events.retain(|e| {
                !matches!(
                    e,
                    Event::Key {
                        key: Key::Space,
                        ..
                    }
                )
            });
            keys
        });
        if space {
            self.primary_action();
        }
        if b {
            self.game.bet_one();
        }
        if m {
            self.game.bet_max();
        }
        for (i, pressed) in nums.iter().enumerate() {
            if *pressed {
                self.toggle_hold(i);
            }
        }
    }

    fn track_changes(&mut self, now: f64) {
        let scale = self.settings.speed.scale();
        let flip_secs = self.flip_secs();
        let mut ticked_now = false;
        for i in 0..5 {
            if self.game.card_gen[i] != self.seen_gen[i] {
                self.seen_gen[i] = self.game.card_gen[i];
                let stagger = i as f64 * FLIP_STAGGER * scale;
                self.flip_start[i] = now + stagger;
                self.prev_face[i] = self.shown[i];
                // Tick as the face comes up; with no animation, tick once for all cards.
                let delay = stagger + flip_secs / 2.0;
                let duplicate = delay == 0.0 && ticked_now;
                if !self.settings.muted && !duplicate {
                    self.sound.flip(delay);
                    ticked_now |= delay == 0.0;
                }
            }
        }
        if self.game.phase != self.seen_phase {
            self.seen_phase = self.game.phase;
            if self.game.phase == Phase::Ready {
                self.result_time = self
                    .flip_start
                    .iter()
                    .fold(now, |end, &start| end.max(start + flip_secs));
                if !self.settings.muted && self.game.result.is_some() {
                    let multiple = self.game.last_win_coins / self.game.coins as u32;
                    self.sound.win(multiple, self.result_time - now);
                }
            }
        }
    }
}

/// Zoom in steps of 1/8 so the layout fills large windows; stepping keeps the font
/// atlas from being rebuilt on every frame of a window resize.
fn fit_zoom(ctx: &Context) {
    let zoom = ctx.zoom_factor();
    let size = ctx.content_rect().size() * zoom;
    let fit = (size.x / DESIGN_SIZE.x).min(size.y / DESIGN_SIZE.y);
    let target = ((fit * 8.0).floor() / 8.0).clamp(1.0, 3.0);
    if target != zoom {
        ctx.set_zoom_factor(target);
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SAVE_KEY, &self.game.saved());
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }

    fn ui(&mut self, root: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        fit_zoom(&ctx);
        if let Some(action) = self.queued.take() {
            self.apply(action);
        }
        self.handle_keys(&ctx);
        let now = ctx.input(|i| i.time);
        self.track_changes(now);
        let revealed = now >= self.result_time;

        CentralPanel::default().frame(Frame::NONE).show(root, |ui| {
            paint_background(ui);
            let full = ui.max_rect();
            let width = (full.width() - 40.0).min(CONTENT_MAX_W);
            let content = Rect::from_min_size(
                pos2(full.center().x - width / 2.0, full.top() + 18.0),
                vec2(width, full.height() - 36.0),
            );
            // Cards take the height the fixed-size parts leave, up to their width limit;
            // any spare height is split around the banner, cards and feedback.
            let flex = content.height() - self.fixed_h;
            let card_w = ((width - CARD_GAP * 4.0) / 5.0)
                .min(CARD_MAX_W)
                .min((flex - TAG_H) / 1.42)
                .max(CARD_MIN_H / 1.42);
            let cards_h = TAG_H + card_w * 1.42;
            let spare = (flex - cards_h).max(0.0);

            ui.scope_builder(UiBuilder::new().max_rect(content), |ui| {
                let mut warps = Vec::new();
                self.header(ui);
                ui.add_space(12.0);
                self.paytable(ui, now, revealed);
                ui.add_space(10.0 + spare / 2.0);
                self.banner(ui, now, revealed, &mut warps);
                ui.add_space(4.0);
                self.cards(ui, now, card_w, &mut warps);
                ui.add_space(8.0);
                self.feedback(ui, revealed);
                ui.add_space(10.0 + spare / 2.0);
                self.controls(ui, now, revealed);
                // Tessellate last, once every glyph used this frame is in the font atlas.
                for w in warps {
                    paint_warp(ui.painter(), w);
                }

                let fixed_h = ui.min_rect().height() - cards_h - spare;
                if (fixed_h - self.fixed_h).abs() > 0.5 {
                    self.fixed_h = fixed_h;
                    ui.ctx().request_discard("measured layout");
                }
            });
        });
    }
}

impl App {
    fn header(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("JACKS OR BETTER")
                    .font(bold(26.0))
                    .color(TEXT),
            );
            ui.add_space(4.0);
            let paytable = badge(ui, "9 / 6", GOLD, DARK);
            paytable.widget_info(|| {
                WidgetInfo::labeled(
                    WidgetType::Label,
                    true,
                    "9/6 paytable: Full House pays 9, Flush pays 6",
                )
            });
            paytable.on_hover_text(
                "Full House pays 9 and Flush pays 6 per coin: \
                 the best common Jacks or Better paytable (99.54% return with perfect play).",
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                self.settings_menu(ui);
                ui.add_space(4.0);
                self.denom_picker(ui);
                ui.label(RichText::new("DENOM").font(bold(12.0)).color(MUTED));
            });
        });
    }

    fn settings_menu(&mut self, ui: &mut Ui) {
        let ready = self.game.phase == Phase::Ready;
        ui.scope(|ui| {
            panel_widget_style(ui);
            let menu = ui.menu_button(RichText::new("⚙").font(reg(15.0)).color(TEXT), |ui| {
                ui.set_min_width(210.0);
                let mut sound = !self.settings.muted;
                ui.checkbox(&mut sound, "Sound");
                self.settings.muted = !sound;
                ui.add_space(4.0);
                ui.label(RichText::new("Animation speed").color(MUTED));
                ui.horizontal(|ui| {
                    for s in Speed::ALL {
                        ui.selectable_value(&mut self.settings.speed, s, s.label());
                    }
                });
                ui.separator();
                if ui
                    .add_enabled(ready, Button::new("Reset bankroll to $100"))
                    .clicked()
                {
                    self.game.reset();
                    ui.close();
                }
                if ui
                    .add_enabled(self.game.decisions > 0, Button::new("Reset accuracy stats"))
                    .clicked()
                {
                    self.game.reset_stats();
                    ui.close();
                }
                ui.separator();
                ui.label(
                    RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                        .small()
                        .color(MUTED),
                );
            });
            menu.response
                .widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Settings"));
            menu.response.on_hover_text("Settings");
        });
    }

    fn denom_picker(&mut self, ui: &mut Ui) {
        let denom = self.game.denom;
        let mut picked = denom;
        let enabled = self.game.phase == Phase::Ready;
        ui.add_enabled_ui(enabled, |ui| {
            panel_widget_style(ui);
            let combo = ComboBox::from_id_salt("denom")
                .width(64.0)
                .selected_text(
                    RichText::new(denom_label(denom))
                        .font(bold(15.0))
                        .color(TEXT),
                )
                .show_ui(ui, |ui| {
                    for d in DENOMS {
                        ui.selectable_value(
                            &mut picked,
                            d,
                            RichText::new(denom_label(d)).font(bold(15.0)),
                        );
                    }
                })
                .response;
            combo.widget_info(|| {
                WidgetInfo::labeled(
                    WidgetType::ComboBox,
                    enabled,
                    format!("Denomination, {}", denom_label(denom)),
                )
            });
            combo.on_hover_text("Coin value. Can be changed between hands.");
        });
        if picked != denom {
            self.game.set_denom(picked);
        }
    }

    fn paytable(&self, ui: &mut Ui, now: f64, revealed: bool) {
        let (rect, resp) =
            ui.allocate_exact_size(vec2(ui.available_width(), TABLE_H), Sense::hover());
        let coins = self.game.coins;
        resp.widget_info(|| {
            let rows: Vec<String> = HandRank::ALL_DESC
                .iter()
                .map(|r| format!("{} {}", r.name(), r.payout(coins)))
                .collect();
            WidgetInfo::labeled(
                WidgetType::Label,
                true,
                format!("Paytable at {coins} coins: {}", rows.join(", ")),
            )
        });
        let p = ui.painter();
        p.rect(
            rect,
            CornerRadius::same(14),
            PANEL,
            Stroke::new(1.0, PANEL_STROKE),
            StrokeKind::Inside,
        );
        let inner = rect.shrink(TABLE_PAD);
        let half_w = inner.width() / 2.0;
        p.vline(
            inner.center().x,
            inner.y_range(),
            Stroke::new(1.0, PANEL_STROKE),
        );
        // Two columns of five rows; the last slot on the right holds the bet note.
        let cell = |i: usize| {
            Rect::from_min_size(
                pos2(
                    inner.left() + half_w * (i / 5) as f32,
                    inner.top() + TABLE_ROW_H * (i % 5) as f32,
                ),
                vec2(half_w, TABLE_ROW_H),
            )
            .shrink2(vec2(8.0, 0.0))
        };

        let since = now - self.result_time;
        let pulsing = since < PULSE_SECS * self.settings.speed.scale();
        let pulse = if pulsing {
            0.55 + 0.45 * ((since * 9.0).sin() as f32).abs()
        } else {
            1.0
        };
        if pulsing {
            ui.ctx().request_repaint();
        }

        for (i, rank) in HandRank::ALL_DESC.iter().enumerate() {
            let row = cell(i);
            let winning =
                revealed && self.game.phase == Phase::Ready && self.game.result == Some(*rank);
            if winning {
                p.rect_filled(row, CornerRadius::same(6), GOLD.gamma_multiply(pulse));
            }
            p.text(
                pos2(row.left() + 10.0, row.center().y),
                Align2::LEFT_CENTER,
                rank.name(),
                bold(15.0),
                if winning { DARK } else { TEXT },
            );
            p.text(
                pos2(row.right() - 10.0, row.center().y),
                Align2::RIGHT_CENTER,
                rank.payout(coins).to_string(),
                bold(15.0),
                if winning { DARK } else { GOLD },
            );
        }

        let bet = format!("BET {coins} × {}", denom_label(self.game.denom));
        let royal_max = HandRank::RoyalFlush.payout(MAX_COINS);
        let (note, color) = if coins == MAX_COINS {
            (format!("{bet}  ·  royal bonus {royal_max}"), GOLD)
        } else {
            (
                format!("{bet}  ·  royal pays {royal_max} at {MAX_COINS} coins"),
                MUTED,
            )
        };
        let slot = cell(9);
        p.text(
            pos2(slot.right() - 10.0, slot.center().y),
            Align2::RIGHT_CENTER,
            note,
            reg(13.0),
            color,
        );
    }

    fn banner(&self, ui: &mut Ui, now: f64, revealed: bool, warps: &mut Vec<Warp>) {
        let g = &self.game;
        let (label, color, font) = if !revealed {
            (String::new(), MUTED, reg(17.0))
        } else if g.is_bust() {
            (
                "Out of credits. Press RESET to reload $100".to_owned(),
                BAD,
                bold(18.0),
            )
        } else {
            match (g.phase, g.result, g.hand) {
                (Phase::Dealt, _, _) => (
                    "Choose cards to HOLD, then DRAW".to_owned(),
                    MUTED,
                    reg(17.0),
                ),
                (Phase::Ready, Some(r), _) => (
                    format!("{}   ·   WIN {}", r.name().to_uppercase(), g.last_win_coins),
                    GOLD,
                    bold(22.0),
                ),
                (Phase::Ready, None, _) if !g.can_deal() => (
                    "Not enough credits for this bet. Lower the bet or denomination".to_owned(),
                    BAD,
                    reg(17.0),
                ),
                (Phase::Ready, None, Some(_)) => {
                    ("No win. DEAL to play again".to_owned(), MUTED, reg(17.0))
                }
                (Phase::Ready, None, None) => {
                    ("Set your bet and press DEAL".to_owned(), MUTED, reg(17.0))
                }
            }
        };
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        resp.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &label));
        let pop = progress(
            now - self.result_time,
            POP_SECS * self.settings.speed.scale(),
        );
        let scale = if g.result.is_some() && now >= self.result_time && pop < 1.0 {
            1.0 + 0.12 * (1.0 - pop)
        } else {
            1.0
        };
        // Scale the laid-out text instead of the font size, so the pop animation
        // doesn't rasterize a new set of glyphs every frame.
        let p = ui.painter();
        let mut shapes = Vec::new();
        text(
            &mut shapes,
            p,
            rect.center(),
            Align2::CENTER_CENTER,
            &label,
            font,
            color,
        );
        warps.push(Warp {
            slot: p.add(Shape::Noop),
            shapes,
            center: rect.center(),
            angle: 0.0,
            depth: 1.0,
            scale,
        });
    }

    fn cards(&mut self, ui: &mut Ui, now: f64, card_w: f32, warps: &mut Vec<Warp>) {
        let card_h = card_w * 1.42;
        let total_w = card_w * 5.0 + CARD_GAP * 4.0;
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), TAG_H + card_h), Sense::hover());
        let x0 = rect.center().x - total_w / 2.0;
        let dealt = self.game.phase == Phase::Dealt;

        for i in 0..5 {
            let crect = Rect::from_min_size(
                pos2(x0 + i as f32 * (card_w + CARD_GAP), rect.top() + TAG_H),
                vec2(card_w, card_h),
            );
            let resp = ui.interact(crect, ui.id().with(("card", i)), Sense::click());
            let held = self.game.held[i];
            let card = self.game.hand.map(|h| h[i]);
            resp.widget_info(|| {
                let name = card.map_or("face down".to_owned(), Card::name);
                WidgetInfo::selected(
                    WidgetType::Checkbox,
                    dealt,
                    held,
                    format!("Hold card {}, {name}", i + 1),
                )
            });
            let hovered = dealt && resp.hovered();
            if resp.clicked() {
                self.toggle_hold(i);
            }
            if dealt {
                resp.on_hover_cursor(CursorIcon::PointingHand);
            }

            let t = progress(now - self.flip_start[i], self.flip_secs());
            if t < 1.0 {
                ui.ctx().request_repaint();
            }
            // Perspective flip about the vertical axis. The left edge swings toward the
            // viewer, so the card leans left while closing and right while reopening.
            // The second half shows the new face turning from edge-on back to flat.
            let theta = smoothstep(t) * PI;
            let (face, angle) = if t >= 0.5 {
                (self.game.hand.map(|h| h[i]), theta - PI)
            } else {
                (self.prev_face[i], theta)
            };
            let lift = if hovered { -4.0 } else { 0.0 };
            let draw_rect = crect.translate(vec2(0.0, lift));
            let held = self.game.held[i];
            let p = ui.painter();
            warps.push(Warp {
                slot: p.add(Shape::Noop),
                shapes: card_shapes(p, draw_rect, face, held, hovered),
                center: draw_rect.center(),
                angle,
                depth: card_w * 2.5,
                scale: 1.0,
            });
            self.shown[i] = face;

            if held {
                // A tab straddling the card's top edge, painted over the card.
                let pill = Rect::from_center_size(
                    pos2(draw_rect.center().x, draw_rect.top() - 3.0),
                    vec2(card_w * 0.42, 20.0),
                );
                p.rect_filled(pill, CornerRadius::same(10), GOLD);
                p.text(
                    pill.center(),
                    Align2::CENTER_CENTER,
                    "HELD",
                    bold(12.0),
                    DARK,
                );
            }
        }
    }

    /// The graded hand as mini cards: green outlines mark the best hold, gold bars the
    /// player's, with the expected return of each beside them.
    fn feedback(&self, ui: &mut Ui, revealed: bool) {
        let (rect, resp) =
            ui.allocate_exact_size(vec2(ui.available_width(), FEEDBACK_H), Sense::hover());
        let Some(fb) = self.game.feedback.as_ref().filter(|_| revealed) else {
            return;
        };
        resp.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, feedback_speech(fb)));
        let p = ui.painter();
        let galley = p.layout_job(feedback_job(fb));
        let mini_h = MINI_W * 1.42;
        let minis_w = MINI_W * 5.0 + 6.0 * 4.0;
        let total_w = minis_w + 20.0 + galley.size().x;
        let x0 = rect.center().x - total_w / 2.0;
        let top = rect.top() + 2.0;

        let mut shapes = Vec::new();
        for (i, &card) in fb.dealt.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(x0 + i as f32 * (MINI_W + 6.0), top),
                vec2(MINI_W, mini_h),
            );
            let cr = CornerRadius::same(4);
            face_shapes(&mut shapes, p, r, cr, card);
            if fb.best_mask & (1 << i) != 0 {
                shapes.push(Shape::rect_stroke(
                    r,
                    cr,
                    Stroke::new(2.5, GREEN),
                    StrokeKind::Outside,
                ));
            }
            if fb.player_mask & (1 << i) != 0 {
                let bar = Rect::from_min_size(
                    pos2(r.left() + 4.0, r.bottom() + 6.0),
                    vec2(MINI_W - 8.0, 4.0),
                );
                shapes.push(Shape::rect_filled(bar, CornerRadius::same(2), GOLD));
            }
        }
        p.extend(shapes);
        let text_pos = pos2(
            x0 + minis_w + 20.0,
            top + mini_h / 2.0 - galley.size().y / 2.0,
        );
        p.galley(text_pos, galley, TEXT);
    }

    fn controls(&mut self, ui: &mut Ui, now: f64, revealed: bool) {
        let g = &self.game;
        let ready = g.phase == Phase::Ready;
        // Count the win up once the drawn cards are face up; before that, show the
        // totals from before the payout.
        let k = if revealed {
            let t = progress(
                now - self.result_time,
                COUNT_SECS * self.settings.speed.scale(),
            );
            if t < 1.0 && g.last_win > 0 {
                ui.ctx().request_repaint();
            }
            smoothstep(t) as f64
        } else {
            0.0
        };
        let win = (g.last_win as f64 * k).round() as u64;
        let win_coins = (g.last_win_coins as f64 * k).round() as u32;
        let bankroll = g.bankroll - g.last_win + win;
        let credits = bankroll / g.denom as u64;
        let (accuracy, record) = if g.decisions == 0 {
            ("–".to_owned(), "no hands yet".to_owned())
        } else {
            (
                format!("{:.0}%", 100.0 * g.correct as f64 / g.decisions as f64),
                format!("{}/{}  ·  streak {}", g.correct, g.decisions, g.streak),
            )
        };

        ui.horizontal(|ui| {
            stat_tile(ui, "CREDITS", &credits.to_string(), &dollars(bankroll));
            stat_tile(ui, "ACCURACY", &accuracy, &record)
                .on_hover_text("Holds that matched optimal strategy, and your current run.");
            stat_tile(ui, "WIN", &win_coins.to_string(), &dollars(win));

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (label, enabled) = if revealed && self.game.is_bust() {
                    ("RESET $100", true)
                } else if self.game.phase == Phase::Dealt {
                    ("DRAW", true)
                } else {
                    ("DEAL", self.game.can_deal())
                };
                let primary = action_button(ui, label, GOLD, DARK, enabled).clicked();
                let bet_max = action_button(ui, "DEAL MAX", PANEL, TEXT, self.game.can_bet_max())
                    .on_hover_text("Bet 5 coins and deal (M)")
                    .clicked();
                let bet_one = action_button(ui, "BET ONE", PANEL, TEXT, ready)
                    .on_hover_text("Add a coin to the bet, 1 to 5 (B)")
                    .clicked();
                if primary {
                    self.queued = Some(Action::Primary);
                }
                if bet_max {
                    self.queued = Some(Action::BetMax);
                }
                if primary || bet_max {
                    ui.ctx().request_repaint();
                }
                if bet_one {
                    self.game.bet_one();
                }
            });
        });

        ui.add_space(4.0);
        ui.label(
            RichText::new("SPACE deal / draw   ·   1-5 hold   ·   B bet one   ·   M deal max")
                .font(reg(12.0))
                .color(MUTED),
        );
    }
}

/// The feedback as a sentence for screen readers.
fn feedback_speech(fb: &Feedback) -> String {
    let hold = |mask: u8| {
        let names: Vec<String> = (0..5)
            .filter(|i| mask & (1 << i) != 0)
            .map(|i| fb.dealt[i].name())
            .collect();
        if names.is_empty() {
            "draw five new cards".to_owned()
        } else {
            format!("hold {}", names.join(", "))
        }
    };
    if fb.optimal {
        format!(
            "Optimal play: {}, returning {:.2} per coin.",
            hold(fb.player_mask),
            fb.player_ev
        )
    } else {
        format!(
            "Better play: {}, returns {:.2} per coin. You chose to {}, returning {:.2}.",
            hold(fb.best_mask),
            fb.best_ev,
            hold(fb.player_mask),
            fb.player_ev
        )
    }
}

fn feedback_job(fb: &Feedback) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut push = |text: &str, font: FontId, color: Color32| {
        job.append(
            text,
            0.0,
            TextFormat {
                font_id: font,
                color,
                line_height: Some(19.0),
                ..Default::default()
            },
        );
    };
    if fb.optimal {
        push("OPTIMAL PLAY\n", bold(13.0), GREEN);
        let what = if fb.player_mask == 0 {
            "Drawing five new cards"
        } else {
            "Your hold"
        };
        push(&format!("{what} was the best play,\n"), reg(14.0), TEXT);
        push(
            &format!("returning {:.2} per coin.", fb.player_ev),
            reg(14.0),
            MUTED,
        );
    } else {
        push("BETTER PLAY\n", bold(13.0), AMBER);
        if fb.best_mask == 0 {
            push("Best: draw five new cards", reg(14.0), TEXT);
        } else {
            push("Best hold ", reg(14.0), TEXT);
            push("(green)", bold(14.0), GREEN);
        }
        push(
            &format!(" returns {:.2} per coin\n", fb.best_ev),
            reg(14.0),
            MUTED,
        );
        if fb.player_mask == 0 {
            push("You drew five new cards,", reg(14.0), TEXT);
        } else {
            push("Your hold ", reg(14.0), TEXT);
            push("(gold)", bold(14.0), GOLD);
        }
        push(&format!(" returns {:.2}", fb.player_ev), reg(14.0), MUTED);
    }
    job
}

/// Styles egui's built-in widgets (combo box, menu button) like the app's panels.
fn panel_widget_style(ui: &mut Ui) {
    // Same padding and height for every such widget, so they line up in a row.
    ui.spacing_mut().button_padding = vec2(12.0, 6.0);
    ui.spacing_mut().interact_size.y = 32.0;
    let w = &mut ui.visuals_mut().widgets;
    for (state, fill) in [
        (&mut w.inactive, PANEL),
        (&mut w.hovered, PANEL_HOVER),
        (&mut w.active, PANEL_HOVER),
        (&mut w.open, PANEL_HOVER),
    ] {
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, PANEL_STROKE);
        state.corner_radius = CornerRadius::same(10);
        state.fg_stroke.color = TEXT;
        state.expansion = 0.0;
    }
}

fn paint_background(ui: &Ui) {
    let r = ui.ctx().content_rect();
    let mut mesh = Mesh::default();
    mesh.colored_vertex(r.left_top(), BG_TOP);
    mesh.colored_vertex(r.right_top(), BG_TOP);
    mesh.colored_vertex(r.right_bottom(), BG_BOTTOM);
    mesh.colored_vertex(r.left_bottom(), BG_BOTTOM);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(mesh));
}

fn badge(ui: &mut Ui, text: &str, fill: Color32, color: Color32) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), bold(13.0), color);
    let size = galley.size() + vec2(16.0, 8.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

fn stat_tile(ui: &mut Ui, label: &str, value: &str, sub: &str) -> Response {
    Frame::NONE
        .fill(PANEL)
        .stroke(Stroke::new(1.0, PANEL_STROKE))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_min_width(80.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(RichText::new(label).font(bold(11.0)).color(MUTED));
                ui.label(RichText::new(value).font(bold(24.0)).color(TEXT));
                ui.label(RichText::new(sub).font(reg(12.0)).color(MUTED));
            });
        })
        .response
}

fn action_button(
    ui: &mut Ui,
    label: &str,
    fill: Color32,
    color: Color32,
    enabled: bool,
) -> Response {
    let stroke = if fill == GOLD { GOLD } else { PANEL_STROKE };
    let b = Button::new(RichText::new(label).font(bold(15.0)).color(color))
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(CornerRadius::same(10))
        .min_size(vec2(110.0, 46.0));
    ui.add_enabled(enabled, b)
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Tessellates the warp's shapes and moves every vertex, so text and paths squash with
/// the card instead of being clipped by it.
fn paint_warp(p: &Painter, w: Warp) {
    if w.angle == 0.0 && w.scale == 1.0 {
        p.set(w.slot, Shape::Vec(w.shapes));
        return;
    }
    let font_tex_size = p.fonts(|f| f.font_image_size());
    let options = p.ctx().tessellation_options(|o| *o);
    let mut tess = Tessellator::new(p.pixels_per_point(), options, font_tex_size, Vec::new());
    let mut mesh = Mesh::default();
    for shape in w.shapes {
        tess.tessellate_shape(shape, &mut mesh);
    }
    let (sin, cos) = w.angle.sin_cos();
    for v in &mut mesh.vertices {
        let d = v.pos - w.center;
        // Rotating by `angle` puts x at depth -x·sin toward the viewer.
        let k = w.scale * w.depth / (w.depth + d.x * sin);
        v.pos = w.center + vec2(d.x * cos, d.y) * k;
    }
    p.set(w.slot, Shape::mesh(mesh));
}

fn text(
    out: &mut Vec<Shape>,
    p: &Painter,
    pos: Pos2,
    anchor: Align2,
    text: &str,
    font: FontId,
    color: Color32,
) -> Rect {
    let galley = p.layout_no_wrap(text.to_owned(), font, color);
    let rect = anchor.anchor_size(pos, galley.size());
    out.push(Shape::galley(rect.min, galley, color));
    rect
}

fn card_shapes(
    p: &Painter,
    rect: Rect,
    card: Option<Card>,
    held: bool,
    hovered: bool,
) -> Vec<Shape> {
    let mut out = Vec::new();
    let cr = CornerRadius::same((rect.height() * 0.06) as u8);
    let shadow = Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(150),
    };
    out.push(shadow.as_shape(rect, cr).into());
    match card {
        Some(c) => face_shapes(&mut out, p, rect, cr, c),
        None => back_shapes(&mut out, p, rect, cr),
    }
    if held {
        out.push(Shape::rect_stroke(
            rect,
            cr,
            Stroke::new(3.0, GOLD),
            StrokeKind::Inside,
        ));
    } else if hovered {
        out.push(Shape::rect_stroke(
            rect,
            cr,
            Stroke::new(2.0, Color32::from_white_alpha(140)),
            StrokeKind::Inside,
        ));
    }
    out
}

fn face_shapes(out: &mut Vec<Shape>, p: &Painter, rect: Rect, cr: CornerRadius, card: Card) {
    out.push(Shape::rect_filled(rect, cr, CARD));
    let w = rect.width();
    let color = if card.suit.is_red() { RED } else { BLACK };
    let pad = w * 0.09;
    // Whole-point font sizes, so resizing the window doesn't fill the glyph atlas.
    let rank_rect = text(
        out,
        p,
        rect.min + vec2(pad, pad * 0.6),
        Align2::LEFT_TOP,
        card.rank_label(),
        bold((w * 0.30).round()),
        color,
    );
    suit_shapes(
        out,
        pos2(rect.min.x + pad + w * 0.085, rank_rect.bottom() + w * 0.11),
        w * 0.17,
        card.suit,
        color,
    );
    suit_shapes(
        out,
        rect.center() + vec2(0.0, w * 0.14),
        w * 0.52,
        card.suit,
        color,
    );
    if (11..=13).contains(&card.rank) {
        // Court cards get a gold inner frame.
        let inset = w * 0.045;
        out.push(Shape::rect_stroke(
            rect.shrink(inset),
            CornerRadius::same(cr.nw.saturating_sub(inset as u8)),
            Stroke::new((w * 0.012).max(1.0), COURT_GOLD),
            StrokeKind::Inside,
        ));
    }
}

fn back_shapes(out: &mut Vec<Shape>, p: &Painter, rect: Rect, cr: CornerRadius) {
    out.push(Shape::rect_filled(rect, cr, CARD));
    let inner = rect.shrink(rect.height() * 0.045);
    let inner_cr = CornerRadius::same((rect.height() * 0.035) as u8);
    out.push(Shape::rect_filled(inner, inner_cr, CARD_BACK));
    let stroke = Stroke::new(1.0, Color32::from_white_alpha(26));
    let step = rect.height() * 0.09;
    let h = inner.height();
    // 45° lines spanning the panel's height, trimmed to its sides.
    let mut x = inner.left() - h;
    while x < inner.right() {
        let t0 = ((inner.left() - x) / h).max(0.0);
        let t1 = ((inner.right() - x) / h).min(1.0);
        for (a, b) in [
            (pos2(x, inner.bottom()), pos2(x + h, inner.top())),
            (pos2(x, inner.top()), pos2(x + h, inner.bottom())),
        ] {
            out.push(Shape::line_segment([a.lerp(b, t0), a.lerp(b, t1)], stroke));
        }
        x += step;
    }
    let r = rect.height() * 0.13;
    out.push(Shape::circle_filled(inner.center(), r, GOLD));
    text(
        out,
        p,
        inner.center(),
        Align2::CENTER_CENTER,
        "JB",
        bold((r * 0.95).round()),
        DARK,
    );
}

fn poly(out: &mut Vec<Shape>, pts: Vec<Pos2>, color: Color32) {
    out.push(Shape::convex_polygon(pts, color, Stroke::NONE));
}

fn stem(out: &mut Vec<Shape>, c: Pos2, s: f32, color: Color32) {
    poly(
        out,
        vec![
            c + vec2(-0.06 * s, 0.05 * s),
            c + vec2(0.06 * s, 0.05 * s),
            c + vec2(0.20 * s, 0.52 * s),
            c + vec2(-0.20 * s, 0.52 * s),
        ],
        color,
    );
}

fn suit_shapes(out: &mut Vec<Shape>, c: Pos2, s: f32, suit: Suit, color: Color32) {
    let circle = |center: Vec2, r: f32| Shape::circle_filled(c + center * s, r * s, color);
    match suit {
        Suit::Hearts => {
            out.push(circle(vec2(-0.25, -0.17), 0.27));
            out.push(circle(vec2(0.25, -0.17), 0.27));
            poly(
                out,
                vec![
                    c + vec2(-0.515 * s, -0.10 * s),
                    c + vec2(0.515 * s, -0.10 * s),
                    c + vec2(0.0, 0.50 * s),
                ],
                color,
            );
        }
        Suit::Diamonds => poly(
            out,
            vec![
                c + vec2(0.0, -0.52 * s),
                c + vec2(0.40 * s, 0.0),
                c + vec2(0.0, 0.52 * s),
                c + vec2(-0.40 * s, 0.0),
            ],
            color,
        ),
        Suit::Spades => {
            out.push(circle(vec2(-0.25, 0.10), 0.27));
            out.push(circle(vec2(0.25, 0.10), 0.27));
            poly(
                out,
                vec![
                    c + vec2(0.0, -0.52 * s),
                    c + vec2(0.515 * s, 0.03 * s),
                    c + vec2(-0.515 * s, 0.03 * s),
                ],
                color,
            );
            stem(out, c, s, color);
        }
        Suit::Clubs => {
            out.push(circle(vec2(0.0, -0.26), 0.23));
            out.push(circle(vec2(-0.25, 0.06), 0.23));
            out.push(circle(vec2(0.25, 0.06), 0.23));
            stem(out, c, s, color);
        }
    }
}
