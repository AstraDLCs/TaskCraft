//! Avatar identicon (RF-ACT-01): una cuadrícula simétrica de 5×5 derivada del
//! ID del agente, en su color. Dibujado por código, estable entre sesiones.

use eframe::egui::{self, Color32, Rect, Sense, Ui, Vec2, pos2};

fn hash(seed: &str) -> u64 {
    // FNV-1a: suficiente para repartir patrones, no es criptográfico.
    seed.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

pub fn avatar(ui: &mut Ui, seed: &str, color: Color32, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint(ui.painter(), rect, seed, color);
    response
}

pub fn paint(painter: &egui::Painter, rect: Rect, seed: &str, color: Color32) {
    let bg = color.gamma_multiply(0.22);
    painter.rect_filled(rect, rect.width() * 0.25, bg);
    let bits = hash(seed);
    let inner = rect.shrink(rect.width() * 0.15);
    let cell = inner.width() / 5.0;
    for row in 0..5u32 {
        for col in 0..3u32 {
            if bits >> (row * 3 + col) & 1 == 1 {
                for c in [col, 4 - col] {
                    let min = pos2(
                        inner.left() + cell * c as f32,
                        inner.top() + cell * row as f32,
                    );
                    painter.rect_filled(
                        Rect::from_min_size(min, Vec2::splat(cell)),
                        cell * 0.2,
                        color,
                    );
                }
            }
        }
    }
}
