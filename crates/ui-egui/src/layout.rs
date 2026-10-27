//! Reparto de la ventana según su ancho, de los 640 px mínimos a pantallas
//! grandes: la vista central conserva siempre un ancho útil, y lo que no
//! cabe a su lado (barra lateral, actividad, detalle) se pliega o se abre
//! encima.

/// Barra lateral completa y plegada (solo iconos).
pub const SIDEBAR: f32 = 230.0;
pub const RAIL: f32 = 46.0;
pub const ACTIVITY: f32 = 280.0;
const DETAIL_MIN: f32 = 340.0;
const DETAIL_MAX: f32 = 520.0;
/// Por debajo de este ancho de ventana, la barra lateral se pliega sola.
pub const SIDEBAR_FROM: f32 = 1100.0;
/// Lo que se deja a la vista central junto al detalle y junto a la actividad.
const CENTER_WITH_DETAIL: f32 = 440.0;
const CENTER_WITH_ACTIVITY: f32 = 720.0;
/// Por debajo, la cabecera de las vistas de tareas se compacta.
pub const COMPACT_CENTER: f32 = 900.0;

/// Lo que ocupa una barra de desplazamiento siempre visible, también cuando
/// el estilo las hace flotantes: lo que llena la vista se lo descuenta.
pub fn scrollbar(ui: &eframe::egui::Ui) -> f32 {
    let s = &ui.spacing().scroll;
    s.bar_width + s.bar_inner_margin + s.bar_outer_margin
}

/// Dónde va el detalle de la tarea abierta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DetailPlace {
    /// Panel a la derecha: ancho inicial y máximo (sin ahogar la vista).
    Side { width: f32, max: f32 },
    /// Ocupa la vista central, con un botón para volver.
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub sidebar_full: bool,
    /// La actividad cabe como panel; si no, se abre encima a petición.
    pub activity_side: bool,
    pub detail: Option<DetailPlace>,
}

/// `width`: ancho de la ventana; `scale`: el del tamaño de texto elegido.
pub fn plan(
    width: f32,
    scale: f32,
    sidebar_collapsed: bool,
    show_activity: bool,
    detail_open: bool,
) -> Plan {
    let w = width / scale.max(0.5);
    let sidebar_full = !sidebar_collapsed && w >= SIDEBAR_FROM;
    let mut rest = w - if sidebar_full { SIDEBAR } else { RAIL };
    let detail = detail_open.then(|| {
        let side = (rest * 0.4).clamp(DETAIL_MIN, DETAIL_MAX);
        if rest - side >= CENTER_WITH_DETAIL {
            let max = rest - CENTER_WITH_DETAIL;
            rest -= side;
            DetailPlace::Side {
                width: side * scale,
                max: max * scale,
            }
        } else {
            DetailPlace::Full
        }
    });
    let activity_side = show_activity
        && detail != Some(DetailPlace::Full)
        && rest - ACTIVITY >= CENTER_WITH_ACTIVITY;
    Plan {
        sidebar_full,
        activity_side,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_minimum_window_keeps_only_the_view() {
        let p = plan(640.0, 1.0, false, true, false);
        assert!(!p.sidebar_full && !p.activity_side);
        let p = plan(640.0, 1.0, false, true, true);
        assert_eq!(p.detail, Some(DetailPlace::Full));
    }

    #[test]
    fn a_laptop_shows_the_detail_beside_the_view() {
        let p = plan(1024.0, 1.0, false, true, true);
        assert!(!p.sidebar_full);
        assert!(matches!(p.detail, Some(DetailPlace::Side { width, .. }) if width >= DETAIL_MIN));
        assert!(!plan(1024.0, 1.0, false, true, false).activity_side);
    }

    #[test]
    fn wide_windows_fit_everything() {
        let p = plan(1440.0, 1.0, false, true, false);
        assert!(p.sidebar_full && p.activity_side);
        let p = plan(1440.0, 1.0, false, true, true);
        assert!(!p.activity_side, "el detalle tiene prioridad");
        let p = plan(1920.0, 1.0, false, true, true);
        assert!(p.sidebar_full && p.activity_side);
        assert!(matches!(p.detail, Some(DetailPlace::Side { width, .. }) if width == DETAIL_MAX));
    }

    #[test]
    fn bigger_text_needs_a_wider_window() {
        assert!(plan(1200.0, 1.0, false, true, false).sidebar_full);
        assert!(!plan(1200.0, 1.3, false, true, false).sidebar_full);
        assert!(!plan(1920.0, 1.0, true, true, false).sidebar_full);
    }
}
