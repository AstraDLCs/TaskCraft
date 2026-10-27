//! Iconos dibujados por código: nítidos a cualquier DPI y teñidos por el tema.

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Plus,
    Trash,
    ChevronRight,
    ChevronDown,
    ChevronUp,
    External,
    Menu,
    Left,
    Right,
    List,
    Board,
    Calendar,
    Review,
    Inbox,
    Agents,
    Search,
    Lock,
    Warning,
    Close,
    Comment,
    Clock,
    Edit,
    Attachment,
    Epic,
    Chart,
    Play,
    Stop,
    Sun,
    Table,
    Gantt,
    Repeat,
    Hourglass,
    Matrix,
    Graph,
    Cycle,
    Milestone,
    Bookmark,
    Diagram,
}

fn poly(p: &Painter, points: &[Pos2], stroke: Stroke) {
    for pair in points.windows(2) {
        if let [a, b] = pair {
            p.line_segment([*a, *b], stroke);
        }
    }
}

pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let w = rect.width();
    let stroke = Stroke::new((w * 0.09).max(1.2), color);
    let r = rect.shrink(w * 0.2);
    let at = |x: f32, y: f32| pos2(r.left() + r.width() * x, r.top() + r.height() * y);
    match icon {
        Icon::Plus => {
            p.line_segment([at(0.5, 0.0), at(0.5, 1.0)], stroke);
            p.line_segment([at(0.0, 0.5), at(1.0, 0.5)], stroke);
        }
        Icon::Close => {
            p.line_segment([at(0.1, 0.1), at(0.9, 0.9)], stroke);
            p.line_segment([at(0.9, 0.1), at(0.1, 0.9)], stroke);
        }
        Icon::ChevronRight | Icon::Right => {
            poly(p, &[at(0.35, 0.1), at(0.75, 0.5), at(0.35, 0.9)], stroke)
        }
        Icon::Left => poly(p, &[at(0.65, 0.1), at(0.25, 0.5), at(0.65, 0.9)], stroke),
        Icon::ChevronDown => poly(p, &[at(0.1, 0.35), at(0.5, 0.75), at(0.9, 0.35)], stroke),
        Icon::ChevronUp => poly(p, &[at(0.1, 0.65), at(0.5, 0.25), at(0.9, 0.65)], stroke),
        Icon::Menu => {
            for y in [0.15, 0.5, 0.85] {
                p.line_segment([at(0.0, y), at(1.0, y)], stroke);
            }
        }
        Icon::External => {
            poly(
                p,
                &[
                    at(0.45, 0.1),
                    at(0.1, 0.1),
                    at(0.1, 0.9),
                    at(0.9, 0.9),
                    at(0.9, 0.55),
                ],
                stroke,
            );
            p.line_segment([at(0.45, 0.55), at(0.95, 0.05)], stroke);
            poly(p, &[at(0.6, 0.05), at(0.95, 0.05), at(0.95, 0.4)], stroke);
        }
        Icon::Trash => {
            p.line_segment([at(0.0, 0.18), at(1.0, 0.18)], stroke);
            p.line_segment([at(0.35, 0.0), at(0.65, 0.0)], stroke);
            let body = Rect::from_min_max(at(0.12, 0.18), at(0.88, 1.0));
            p.rect_stroke(body, w * 0.08, stroke, egui::StrokeKind::Inside);
        }
        Icon::List => {
            for y in [0.15, 0.5, 0.85] {
                p.circle_filled(at(0.05, y), w * 0.05, color);
                p.line_segment([at(0.25, y), at(1.0, y)], stroke);
            }
        }
        Icon::Board => {
            for (x, h) in [(0.0, 1.0), (0.37, 0.7), (0.74, 0.45)] {
                let col = Rect::from_min_max(at(x, 0.0), at(x + 0.26, h));
                p.rect_stroke(col, w * 0.04, stroke, egui::StrokeKind::Inside);
            }
        }
        Icon::Calendar => {
            let body = Rect::from_min_max(at(0.0, 0.12), at(1.0, 1.0));
            p.rect_stroke(body, w * 0.06, stroke, egui::StrokeKind::Inside);
            p.line_segment([at(0.0, 0.38), at(1.0, 0.38)], stroke);
            p.line_segment([at(0.28, 0.0), at(0.28, 0.22)], stroke);
            p.line_segment([at(0.72, 0.0), at(0.72, 0.22)], stroke);
        }
        Icon::Review => {
            let body = Rect::from_min_max(at(0.0, 0.0), at(0.8, 1.0));
            p.rect_stroke(body, w * 0.05, stroke, egui::StrokeKind::Inside);
            poly(p, &[at(0.15, 0.55), at(0.35, 0.75), at(0.65, 0.3)], stroke);
        }
        Icon::Inbox => {
            poly(
                p,
                &[at(0.0, 0.55), at(0.15, 0.1), at(0.85, 0.1), at(1.0, 0.55)],
                stroke,
            );
            poly(
                p,
                &[
                    at(0.0, 0.55),
                    at(0.0, 1.0),
                    at(1.0, 1.0),
                    at(1.0, 0.55),
                    at(0.68, 0.55),
                    at(0.6, 0.72),
                    at(0.4, 0.72),
                    at(0.32, 0.55),
                    at(0.0, 0.55),
                ],
                stroke,
            );
        }
        Icon::Agents => {
            p.circle_stroke(at(0.5, 0.3), r.width() * 0.25, stroke);
            poly(
                p,
                &[at(0.05, 1.0), at(0.2, 0.68), at(0.8, 0.68), at(0.95, 1.0)],
                stroke,
            );
        }
        Icon::Search => {
            p.circle_stroke(at(0.42, 0.42), r.width() * 0.36, stroke);
            p.line_segment([at(0.7, 0.7), at(1.0, 1.0)], stroke);
        }
        Icon::Lock => {
            let body = Rect::from_min_max(at(0.1, 0.45), at(0.9, 1.0));
            p.rect_filled(body, w * 0.06, color);
            poly(
                p,
                &[
                    at(0.28, 0.45),
                    at(0.28, 0.2),
                    at(0.5, 0.0),
                    at(0.72, 0.2),
                    at(0.72, 0.45),
                ],
                stroke,
            );
        }
        Icon::Warning => {
            poly(
                p,
                &[at(0.5, 0.0), at(1.0, 1.0), at(0.0, 1.0), at(0.5, 0.0)],
                stroke,
            );
            p.line_segment([at(0.5, 0.35), at(0.5, 0.68)], stroke);
            p.circle_filled(at(0.5, 0.84), w * 0.05, color);
        }
        Icon::Comment => {
            poly(
                p,
                &[
                    at(0.0, 0.0),
                    at(1.0, 0.0),
                    at(1.0, 0.7),
                    at(0.4, 0.7),
                    at(0.15, 1.0),
                    at(0.15, 0.7),
                    at(0.0, 0.7),
                    at(0.0, 0.0),
                ],
                stroke,
            );
        }
        Icon::Attachment => {
            // Clip: dos óvalos abiertos encajados.
            poly(
                p,
                &[
                    at(0.75, 0.35),
                    at(0.75, 0.85),
                    at(0.5, 1.0),
                    at(0.25, 0.85),
                    at(0.25, 0.15),
                    at(0.42, 0.0),
                    at(0.6, 0.15),
                    at(0.6, 0.75),
                    at(0.5, 0.82),
                    at(0.4, 0.75),
                    at(0.4, 0.3),
                ],
                stroke,
            );
        }
        Icon::Chart => {
            for (x, h) in [(0.15, 0.45), (0.5, 0.8), (0.85, 0.6)] {
                p.line_segment(
                    [at(x, 1.0), at(x, 1.0 - h)],
                    Stroke::new(stroke.width * 1.6, color),
                );
            }
        }
        Icon::Play => {
            p.add(egui::Shape::convex_polygon(
                vec![at(0.15, 0.0), at(0.95, 0.5), at(0.15, 1.0)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            p.rect_filled(
                Rect::from_min_max(at(0.1, 0.1), at(0.9, 0.9)),
                w * 0.06,
                color,
            );
        }
        Icon::Epic => {
            // Banderín: mástil y paño.
            p.line_segment([at(0.15, 0.0), at(0.15, 1.0)], stroke);
            poly(p, &[at(0.15, 0.05), at(0.9, 0.25), at(0.15, 0.5)], stroke);
        }
        Icon::Cycle => {
            // Dos medias vueltas, cada una con su punta: el ciclo que se repite.
            let c = r.center();
            let radius = r.width() * 0.45;
            for half in [0.0_f32, 0.5] {
                let arc: Vec<Pos2> = (0..=10)
                    .map(|i| {
                        let a = std::f32::consts::TAU * (half + 0.06 + 0.36 * i as f32 / 10.0);
                        pos2(c.x + radius * a.cos(), c.y - radius * a.sin())
                    })
                    .collect();
                poly(p, &arc, stroke);
                if let Some(tip) = arc.last() {
                    let out = (*tip - c).normalized() * w * 0.14;
                    let back = Vec2::new(-out.y, out.x);
                    poly(p, &[*tip + out + back, *tip, *tip - out + back], stroke);
                }
            }
        }
        Icon::Milestone => {
            poly(
                p,
                &[
                    at(0.5, 0.0),
                    at(1.0, 0.5),
                    at(0.5, 1.0),
                    at(0.0, 0.5),
                    at(0.5, 0.0),
                ],
                stroke,
            );
        }
        Icon::Bookmark => {
            poly(
                p,
                &[
                    at(0.2, 0.0),
                    at(0.8, 0.0),
                    at(0.8, 1.0),
                    at(0.5, 0.72),
                    at(0.2, 1.0),
                    at(0.2, 0.0),
                ],
                stroke,
            );
        }
        Icon::Clock => {
            p.circle_stroke(r.center(), r.width() * 0.5, stroke);
            poly(p, &[at(0.5, 0.2), at(0.5, 0.5), at(0.72, 0.62)], stroke);
        }
        Icon::Edit => {
            poly(
                p,
                &[
                    at(0.0, 1.0),
                    at(0.08, 0.72),
                    at(0.75, 0.05),
                    at(0.95, 0.25),
                    at(0.28, 0.92),
                    at(0.0, 1.0),
                ],
                stroke,
            );
        }
        Icon::Sun => {
            p.circle_stroke(r.center(), r.width() * 0.22, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let (s, c) = a.sin_cos();
                let from = r.center() + egui::vec2(c, s) * r.width() * 0.36;
                let to = r.center() + egui::vec2(c, s) * r.width() * 0.5;
                p.line_segment([from, to], stroke);
            }
        }
        Icon::Table => {
            let body = Rect::from_min_max(at(0.0, 0.0), at(1.0, 1.0));
            p.rect_stroke(body, w * 0.04, stroke, egui::StrokeKind::Inside);
            p.line_segment([at(0.0, 0.33), at(1.0, 0.33)], stroke);
            p.line_segment([at(0.0, 0.66), at(1.0, 0.66)], stroke);
            p.line_segment([at(0.4, 0.0), at(0.4, 1.0)], stroke);
        }
        Icon::Gantt => {
            for (x0, x1, y) in [(0.0, 0.55, 0.15), (0.3, 0.85, 0.5), (0.5, 1.0, 0.85)] {
                p.line_segment(
                    [at(x0, y), at(x1, y)],
                    Stroke::new(stroke.width * 1.8, color),
                );
            }
        }
        Icon::Matrix => {
            p.rect_stroke(
                Rect::from_min_max(at(0.0, 0.0), at(1.0, 1.0)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.line_segment([at(0.5, 0.0), at(0.5, 1.0)], stroke);
            p.line_segment([at(0.0, 0.5), at(1.0, 0.5)], stroke);
        }
        Icon::Diagram => {
            // Diagrama de flujo: una caja arriba que se bifurca en dos abajo.
            let boxed = |x: f32, y: f32| Rect::from_center_size(at(x, y), r.size() * 0.32);
            for b in [boxed(0.5, 0.15), boxed(0.15, 0.85), boxed(0.85, 0.85)] {
                p.rect_stroke(b, w * 0.03, stroke, egui::StrokeKind::Middle);
            }
            poly(
                p,
                &[at(0.5, 0.31), at(0.5, 0.5), at(0.15, 0.5), at(0.15, 0.69)],
                stroke,
            );
            poly(p, &[at(0.5, 0.5), at(0.85, 0.5), at(0.85, 0.69)], stroke);
        }
        Icon::Graph => {
            let (a, b, c) = (at(0.1, 0.5), at(0.9, 0.15), at(0.9, 0.85));
            p.line_segment([a, b], stroke);
            p.line_segment([a, c], stroke);
            for n in [a, b, c] {
                p.circle_filled(n, w * 0.1, color);
            }
        }
        Icon::Hourglass => {
            poly(
                p,
                &[
                    at(0.15, 0.0),
                    at(0.85, 0.0),
                    at(0.15, 1.0),
                    at(0.85, 1.0),
                    at(0.15, 0.0),
                ],
                stroke,
            );
        }
        Icon::Repeat => {
            // Arco de casi una vuelta con punta de flecha al final.
            let c = r.center();
            let radius = r.width() * 0.45;
            let arc: Vec<Pos2> = (0..=20)
                .map(|i| {
                    let a = std::f32::consts::TAU * (0.08 + 0.8 * i as f32 / 20.0);
                    pos2(c.x + radius * a.cos(), c.y - radius * a.sin())
                })
                .collect();
            poly(p, &arc, stroke);
            if let Some(end) = arc.last() {
                let tip = *end;
                poly(
                    p,
                    &[
                        tip + Vec2::new(-w * 0.18, -w * 0.02),
                        tip,
                        tip + Vec2::new(-w * 0.02, -w * 0.18),
                    ],
                    stroke,
                );
            }
        }
    }
}

/// Interruptor con icono: en color cuando está activo.
pub fn toggle(ui: &mut Ui, icon: Icon, on: bool, active: Color32, tooltip: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
    let visuals = ui.style().interact(&response);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, visuals.corner_radius, visuals.bg_fill);
    }
    let color = if on {
        active
    } else {
        ui.visuals().weak_text_color()
    };
    paint(ui.painter(), rect, icon, color);
    response.on_hover_text(tooltip)
}

/// Icono suelto (no interactivo) del tamaño de una línea de texto.
pub fn show(ui: &mut Ui, icon: Icon, color: Color32) -> egui::Response {
    let size = ui.text_style_height(&egui::TextStyle::Body);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint(ui.painter(), rect, icon, color);
    response
}

pub fn button(ui: &mut Ui, icon: Icon, enabled: bool, tooltip: &str) -> egui::Response {
    button_sized(ui, icon, enabled, tooltip, 22.0)
}

pub fn button_sized(
    ui: &mut Ui,
    icon: Icon,
    enabled: bool,
    tooltip: &str,
    size: f32,
) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), sense);
    let visuals = ui.style().interact(&response);
    let color = if enabled {
        visuals.fg_stroke.color
    } else {
        ui.visuals().weak_text_color()
    };
    if enabled && response.hovered() {
        ui.painter()
            .rect_filled(rect, visuals.corner_radius, visuals.bg_fill);
    }
    paint(ui.painter(), rect, icon, color);
    response.on_hover_text(tooltip)
}

/// Pestaña: el icono y, si se da, el texto, dentro de la misma pastilla.
/// Sin texto, el nombre va en el tooltip.
pub fn tab(
    ui: &mut Ui,
    icon: Icon,
    label: Option<&str>,
    selected: bool,
    name: &str,
) -> egui::Response {
    let pad = ui.spacing().button_padding;
    let icon_size = ui.spacing().interact_size.y - 4.0;
    let galley = label.map(|l| {
        ui.painter().layout_no_wrap(
            l.to_owned(),
            egui::TextStyle::Button.resolve(ui.style()),
            egui::Color32::PLACEHOLDER,
        )
    });
    let text_w = galley.as_ref().map_or(0.0, |g| g.size().x + pad.x);
    let size = Vec2::new(
        pad.x * 2.0 + icon_size + text_w,
        ui.spacing().interact_size.y,
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, name));
    let visuals = ui.style().interact_selectable(&response, selected);
    if selected || response.hovered() {
        ui.painter()
            .rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
    }
    let color = if selected {
        visuals.text_color()
    } else {
        ui.visuals().widgets.inactive.text_color()
    };
    let icon_rect = Rect::from_min_size(
        pos2(rect.left() + pad.x, rect.center().y - icon_size / 2.0),
        Vec2::splat(icon_size),
    );
    paint(ui.painter(), icon_rect, icon, color);
    if let Some(g) = galley {
        let pos = pos2(
            icon_rect.right() + pad.x,
            rect.center().y - g.size().y / 2.0,
        );
        ui.painter().galley(pos, g, color);
        response
    } else {
        response.on_hover_text(name)
    }
}

/// Ancho de [`tab`] con o sin texto, para decidir si caben los nombres.
pub fn tab_width(ui: &Ui, label: Option<&str>) -> f32 {
    let pad = ui.spacing().button_padding;
    let icon_size = ui.spacing().interact_size.y - 4.0;
    let text_w = label.map_or(0.0, |l| {
        ui.painter()
            .layout_no_wrap(
                l.to_owned(),
                egui::TextStyle::Button.resolve(ui.style()),
                egui::Color32::PLACEHOLDER,
            )
            .size()
            .x
            + pad.x
    });
    pad.x * 2.0 + icon_size + text_w + ui.spacing().item_spacing.x
}

/// Punto de color, para prioridades y estados.
pub fn dot(ui: &mut Ui, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
    response
}
