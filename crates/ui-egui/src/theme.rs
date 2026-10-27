//! Colores y medidas compartidos por todas las vistas.

use eframe::egui::{self, Color32, FontFamily, FontId, TextStyle};
use model::{Priority, Status};

/// Tamaño del texto elegido por la persona; escala todos los estilos a la vez.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextSize {
    Small,
    #[default]
    Normal,
    Large,
    Huge,
}

impl TextSize {
    pub const ALL: [Self; 4] = [Self::Small, Self::Normal, Self::Large, Self::Huge];

    pub fn code(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Normal => "normal",
            Self::Large => "large",
            Self::Huge => "huge",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.code() == code)
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Small => "ui-text-small",
            Self::Normal => "ui-text-normal",
            Self::Large => "ui-text-large",
            Self::Huge => "ui-text-huge",
        }
    }

    pub fn scale(self) -> f32 {
        match self {
            Self::Small => 0.9,
            Self::Normal => 1.0,
            Self::Large => 1.15,
            Self::Huge => 1.3,
        }
    }
}

/// Tamaños base, más grandes que los de egui (su "pequeño" de 9 px no se lee).
pub fn apply_text_size(ctx: &egui::Context, size: TextSize) {
    let k = size.scale();
    let font = |px: f32, family: FontFamily| FontId::new((px * k).round(), family);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, font(12.0, FontFamily::Proportional)),
            (TextStyle::Body, font(15.0, FontFamily::Proportional)),
            (TextStyle::Button, font(15.0, FontFamily::Proportional)),
            (TextStyle::Heading, font(21.0, FontFamily::Proportional)),
            (TextStyle::Monospace, font(14.0, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.interact_size.y = (22.0 * k).round();
        style.spacing.item_spacing = egui::vec2(8.0, 5.0 * k);
        style.spacing.button_padding = egui::vec2(6.0 * k, 3.0 * k);
    });
}

pub const RED: Color32 = Color32::from_rgb(0xe5, 0x48, 0x4d);
pub const ORANGE: Color32 = Color32::from_rgb(0xf7, 0x6b, 0x15);
pub const YELLOW: Color32 = Color32::from_rgb(0xff, 0xc5, 0x3d);
pub const GREEN: Color32 = Color32::from_rgb(0x46, 0xa7, 0x58);
pub const TEAL: Color32 = Color32::from_rgb(0x12, 0xa5, 0x94);
pub const BLUE: Color32 = Color32::from_rgb(0x00, 0x90, 0xff);
pub const VIOLET: Color32 = Color32::from_rgb(0x6e, 0x56, 0xcf);
pub const GRAY: Color32 = Color32::from_rgb(0x8b, 0x8d, 0x98);

pub fn priority(p: Priority) -> Color32 {
    match p {
        Priority::Urgent => RED,
        Priority::High => ORANGE,
        Priority::Medium => YELLOW,
        Priority::Low => BLUE,
        Priority::None => GRAY,
    }
}

pub fn status(s: Status) -> Color32 {
    match s {
        Status::Backlog => GRAY,
        Status::Todo => BLUE,
        Status::InProgress => ORANGE,
        Status::InReview => VIOLET,
        Status::Done => GREEN,
        Status::Cancelled => GRAY,
    }
}

/// `#rrggbb` → color; ilegible → gris.
pub fn parse_hex(hex: &str) -> Color32 {
    let channel = |i: usize| {
        hex.get(i..i + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
            .unwrap_or(0x80)
    };
    Color32::from_rgb(channel(1), channel(3), channel(5))
}

pub fn to_hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Texto legible (negro o blanco) sobre un fondo de color.
pub fn on(bg: Color32) -> Color32 {
    let lum = 0.299 * f32::from(bg.r()) + 0.587 * f32::from(bg.g()) + 0.114 * f32::from(bg.b());
    if lum > 150.0 {
        Color32::from_rgb(0x1a, 0x1a, 0x1a)
    } else {
        Color32::WHITE
    }
}
