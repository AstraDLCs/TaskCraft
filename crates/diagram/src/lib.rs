//! L3 — Diagramas Mermaid (RF-MOD-19, D21): de la fuente a SVG con `merman` y
//! del SVG a píxeles con `resvg`. Sin web ni estado propio, salvo las fuentes
//! tipográficas, que se cargan una vez.
//!
//! El motor se llama dentro de `catch_unwind`, con un plazo y con la fuente y
//! el raster acotados: un diagrama roto nunca tumba al proceso.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use i18n::{Localize, Msg};
use merman::svg::{
    HostTheme, HostThemeAppearance, Presentation, SvgOutputPolicy, SvgPipelinePreset, ThemeRole,
};
use merman::{OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest};
use resvg::{tiny_skia, usvg};

/// Tamaño máximo de la fuente, en bytes.
pub const MAX_SOURCE: usize = 64 * 1024;
/// Lado máximo del raster, en píxeles: si no cabe, se reduce la escala.
pub const MAX_SIDE: u32 = 4096;
/// Plazo para pasar una fuente a SVG.
const DEADLINE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagramError {
    /// La fuente está vacía.
    Empty,
    TooLarge {
        max: usize,
    },
    /// Error de sintaxis o de tipo de diagrama, con el mensaje del motor.
    Syntax(String),
    /// Fallo al dibujar (SVG inválido, plazo agotado, pánico del motor).
    Render(String),
}

impl DiagramError {
    /// Mensaje del motor, sin traducir (los agentes lo reciben tal cual).
    pub fn detail(&self) -> String {
        match self {
            Self::Empty => "empty source".into(),
            Self::TooLarge { max } => format!("source larger than {max} bytes"),
            Self::Syntax(d) | Self::Render(d) => d.clone(),
        }
    }
}

impl Localize for DiagramError {
    fn msg(&self) -> Msg {
        match self {
            Self::Empty => Msg::new("diagram-error-empty"),
            Self::TooLarge { max } => Msg::new("diagram-error-too-large").arg("max", *max),
            Self::Syntax(d) => Msg::new("diagram-error-syntax").arg("detail", d.as_str()),
            Self::Render(d) => Msg::new("diagram-error-render").arg("detail", d.as_str()),
        }
    }
}

/// Imagen RGBA con alfa premultiplicado, fila a fila.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Comprueba que la fuente se puede dibujar.
pub fn check(source: &str) -> Result<(), DiagramError> {
    render_svg(source, Theme::Light).map(|_| ())
}

/// Fuente Mermaid → SVG apto para resvg (sin `foreignObject`), con el fondo
/// transparente.
pub fn render_svg(source: &str, theme: Theme) -> Result<String, DiagramError> {
    if source.trim().is_empty() {
        return Err(DiagramError::Empty);
    }
    if source.len() > MAX_SOURCE {
        return Err(DiagramError::TooLarge { max: MAX_SOURCE });
    }
    let run = || {
        let resolved = Presentation::new().with_theme(host_theme(theme)).resolve();
        let renderer =
            Renderer::new().with_engine(resolved.materialize_engine(merman::Engine::new()));
        let policy = SvgOutputPolicy {
            preset: SvgPipelinePreset::ResvgSafe,
            ..SvgOutputPolicy::default()
        };
        let request = SvgRequest {
            pipeline: Some(policy.pipeline()),
            presentation: resolved.render_policy(),
            ..Default::default()
        };
        let control = OperationControl::new().with_deadline(DEADLINE);
        match renderer.render(RenderRequest::svg(source, control, request)) {
            Ok(RenderOutput::Svg(Some(svg))) => Ok(svg.svg().to_string()),
            Ok(_) => Err(DiagramError::Syntax("no Mermaid diagram detected".into())),
            Err(e) => Err(DiagramError::Syntax(e.to_string())),
        }
    };
    catch_unwind(AssertUnwindSafe(run))
        .unwrap_or_else(|_| Err(DiagramError::Render("renderer panicked".into())))
}

/// SVG → píxeles a `scale` (1.0 = tamaño natural), sin pasar de [`MAX_SIDE`].
pub fn rasterize(svg: &str, scale: f32) -> Result<Image, DiagramError> {
    let run = || {
        let options = usvg::Options {
            fontdb: Arc::clone(fonts()),
            ..usvg::Options::default()
        };
        let tree =
            usvg::Tree::from_str(svg, &options).map_err(|e| DiagramError::Render(e.to_string()))?;
        let size = tree.size();
        let longest = size.width().max(size.height()).max(1.0);
        let scale = scale.clamp(0.1, 8.0).min(MAX_SIDE as f32 / longest);
        let width = ((size.width() * scale).ceil() as u32).clamp(1, MAX_SIDE);
        let height = ((size.height() * scale).ceil() as u32).clamp(1, MAX_SIDE);
        let mut pixmap = tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| DiagramError::Render(format!("cannot allocate {width}x{height}")))?;
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        Ok(Image {
            width,
            height,
            rgba: pixmap.take(),
        })
    };
    catch_unwind(AssertUnwindSafe(run))
        .unwrap_or_else(|_| Err(DiagramError::Render("rasterizer panicked".into())))
}

/// Fuente Mermaid → píxeles.
pub fn render(source: &str, theme: Theme, scale: f32) -> Result<Image, DiagramError> {
    rasterize(&render_svg(source, theme)?, scale)
}

/// Tema claro: el de Mermaid. Oscuro: roles a juego con el tema oscuro de la app.
fn host_theme(theme: Theme) -> HostTheme {
    let base = HostTheme::new();
    match theme {
        Theme::Light => base.with_appearance(HostThemeAppearance::Light),
        Theme::Dark => DARK.iter().fold(
            base.with_appearance(HostThemeAppearance::Dark),
            |t, (role, color)| t.clone().try_with_role(*role, *color).unwrap_or(t),
        ),
    }
}

const DARK: [(ThemeRole, &str); 22] = [
    (ThemeRole::Canvas, "#1b1b1b"),
    (ThemeRole::Surface, "#2a3442"),
    (ThemeRole::SurfaceAlt, "#33302a"),
    (ThemeRole::SurfaceMuted, "#262626"),
    (ThemeRole::Text, "#e6e6e6"),
    (ThemeRole::SubtleText, "#a0a0a0"),
    (ThemeRole::Border, "#6b8bb5"),
    (ThemeRole::Line, "#a8a8a8"),
    (ThemeRole::EdgeLabelBackground, "#262626"),
    (ThemeRole::ClusterBackground, "#232a33"),
    (ThemeRole::ClusterBorder, "#55657a"),
    (ThemeRole::NoteBackground, "#4a3f1c"),
    (ThemeRole::NoteBorder, "#c9a227"),
    (ThemeRole::NoteText, "#f2e6b8"),
    (ThemeRole::ActorBackground, "#2a3442"),
    (ThemeRole::ActorBorder, "#6b8bb5"),
    (ThemeRole::ActorText, "#e6e6e6"),
    (ThemeRole::ActivationBackground, "#33302a"),
    (ThemeRole::ActivationBorder, "#a8a8a8"),
    (ThemeRole::Error, "#e5534b"),
    (ThemeRole::Warning, "#d29922"),
    (ThemeRole::Success, "#57ab5a"),
];

/// Fuentes del sistema más las de egui embebidas, para que el texto se vea
/// aunque el sistema no tenga ninguna. `sans-serif` (el último recurso de
/// Mermaid) apunta a la primera familia conocida que exista.
fn fonts() -> &'static Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        db.load_font_data(epaint_default_fonts::UBUNTU_LIGHT.to_vec());
        db.load_font_data(epaint_default_fonts::HACK_REGULAR.to_vec());
        let has = |name: &str| {
            db.faces()
                .any(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)))
        };
        let sans = [
            "Arial",
            "Liberation Sans",
            "DejaVu Sans",
            "Noto Sans",
            "Segoe UI",
            "Ubuntu",
        ]
        .into_iter()
        .find(|name| has(name))
        .unwrap_or("Ubuntu");
        db.set_sans_serif_family(sans);
        Arc::new(db)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOW: &str = "flowchart TD\n  A[Inicio] --> B{¿Válido?}\n  B -->|sí| C[Guardar]";
    const SEQ: &str = "sequenceDiagram\n  Agente->>Daemon: task.claim\n  Daemon-->>Agente: ok";

    #[test]
    fn valid_sources_become_svg_and_pixels() {
        for theme in [Theme::Light, Theme::Dark] {
            for source in [FLOW, SEQ] {
                let svg = render_svg(source, theme).expect("svg");
                assert!(svg.starts_with("<svg") && !svg.contains("foreignObject"));
                let img = rasterize(&svg, 1.0).expect("raster");
                assert_eq!(img.rgba.len(), (img.width * img.height * 4) as usize);
                assert!(img.rgba.chunks(4).any(|p| p[3] > 0), "dibujó algo");
            }
        }
    }

    #[test]
    fn invalid_sources_are_errors_not_panics() {
        assert_eq!(check("  "), Err(DiagramError::Empty));
        assert!(matches!(
            check(&"x".repeat(MAX_SOURCE + 1)),
            Err(DiagramError::TooLarge { .. })
        ));
        assert!(matches!(check("hola mundo"), Err(DiagramError::Syntax(_))));
        let Err(DiagramError::Syntax(detail)) = check("flowchart TD\n  A --> \n  ][[[") else {
            panic!("se esperaba error de sintaxis");
        };
        assert!(!detail.is_empty());
        assert!(matches!(
            rasterize("<no-es-svg", 1.0),
            Err(DiagramError::Render(_))
        ));
    }

    #[test]
    fn raster_is_bounded() {
        let svg = render_svg(FLOW, Theme::Light).expect("svg");
        let img = rasterize(&svg, 1000.0).expect("raster");
        assert!(img.width <= MAX_SIDE && img.height <= MAX_SIDE);
    }
}
