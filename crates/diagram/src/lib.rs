//! L3 — Diagramas Mermaid (RF-MOD-19, D21): de la fuente a SVG con `merman` y
//! del SVG a píxeles con `resvg`. Sin web ni estado propio, salvo las fuentes
//! tipográficas, que se cargan una vez.
//!
//! El motor se llama dentro de `catch_unwind`, con un plazo y con la fuente y
//! el raster acotados: un diagrama roto nunca tumba al proceso.
//!
//! merman mide el texto con la misma fuente con la que luego dibuja resvg
//! (TC-1): si no, cada etiqueta se recorta a una caja calculada con otra
//! fuente.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use i18n::{Localize, Msg};
use merman::svg::{
    DeterministicTextMeasurer, HostTheme, HostThemeAppearance, MeasurementProfileId, Presentation,
    SvgOutputPolicy, SvgPipelinePreset, TextMeasurementPolicy, TextMeasurementProfile,
    TextMeasurementProfileIdentity, TextStyle, ThemeRole,
};
use merman::{OperationControl, RenderOutput, RenderRequest, Renderer, SvgEnvironment, SvgRequest};
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
        let fonts = fonts();
        let resolved = Presentation::new()
            .with_theme(host_theme(theme, &fonts.family))
            .resolve();
        let renderer =
            Renderer::new().with_engine(resolved.materialize_engine(merman::Engine::new()));
        let policy = SvgOutputPolicy {
            preset: SvgPipelinePreset::ResvgSafe,
            ..SvgOutputPolicy::default()
        };
        let mut environment = SvgEnvironment::deterministic();
        if let Some(measurement) = measurement_policy(fonts) {
            environment = environment.with_text_measurement_policy(measurement);
        }
        let request = SvgRequest {
            environment,
            pipeline: Some(policy.pipeline()),
            presentation: resolved.render_policy(),
            ..Default::default()
        };
        let control = OperationControl::new().with_deadline(DEADLINE);
        match renderer.render(RenderRequest::svg(source, control, request)) {
            Ok(RenderOutput::Svg(Some(svg))) => Ok(clean_svg(svg.svg())),
            Ok(_) => Err(DiagramError::Syntax("no Mermaid diagram detected".into())),
            Err(e) => Err(DiagramError::Syntax(e.to_string())),
        }
    };
    catch_unwind(AssertUnwindSafe(run))
        .unwrap_or_else(|_| Err(DiagramError::Render("renderer panicked".into())))
}

/// Un SVG ya interpretado: se dibuja a cualquier escala sin volver a pasar la
/// fuente por merman, que es lo caro (el visor rasteriza un escalón por zoom).
pub struct Drawing {
    tree: usvg::Tree,
}

impl Drawing {
    /// Fuente Mermaid → dibujo.
    pub fn new(source: &str, theme: Theme) -> Result<Self, DiagramError> {
        Self::parse(&render_svg(source, theme)?)
    }

    /// SVG → dibujo.
    pub fn parse(svg: &str) -> Result<Self, DiagramError> {
        let run = || {
            let options = usvg::Options {
                fontdb: Arc::clone(&fonts().db),
                ..usvg::Options::default()
            };
            usvg::Tree::from_str(svg, &options)
                .map(|tree| Self { tree })
                .map_err(|e| DiagramError::Render(e.to_string()))
        };
        catch_unwind(AssertUnwindSafe(run))
            .unwrap_or_else(|_| Err(DiagramError::Render("svg parser panicked".into())))
    }

    /// Ancho y alto a escala 1.
    pub fn size(&self) -> (f32, f32) {
        let size = self.tree.size();
        (size.width(), size.height())
    }

    /// Píxeles a `scale` (1.0 = tamaño natural), sin pasar de [`MAX_SIDE`].
    pub fn rasterize(&self, scale: f32) -> Result<Image, DiagramError> {
        let run = || {
            let (w, h) = self.size();
            let scale = scale
                .clamp(0.1, 8.0)
                .min(MAX_SIDE as f32 / w.max(h).max(1.0));
            let width = ((w * scale).ceil() as u32).clamp(1, MAX_SIDE);
            let height = ((h * scale).ceil() as u32).clamp(1, MAX_SIDE);
            let mut pixmap = tiny_skia::Pixmap::new(width, height)
                .ok_or_else(|| DiagramError::Render(format!("cannot allocate {width}x{height}")))?;
            resvg::render(
                &self.tree,
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
}

/// SVG → píxeles a `scale` (1.0 = tamaño natural), sin pasar de [`MAX_SIDE`].
pub fn rasterize(svg: &str, scale: f32) -> Result<Image, DiagramError> {
    Drawing::parse(svg)?.rasterize(scale)
}

/// Quita del SVG de merman lo que usvg no entiende y solo produce avisos
/// en el log (TC-1): `transform:none` en los estilos en línea y
/// `dominant-baseline:start`, que no es un valor de SVG. Y las sombras de
/// los nodos (TC-3), que multiplican por 10–50 el tiempo de rasterizar.
fn clean_svg(svg: &str) -> String {
    let svg = svg
        .replace(";transform:none", "")
        .replace("transform:none;", "")
        .replace("dominant-baseline:start", "dominant-baseline:auto")
        .replace("dominant-baseline=\"start\"", "dominant-baseline=\"auto\"");
    unfill_flat_paths(&without_shadows(&svg))
}

/// Trazos rectos horizontales o verticales con relleno (las marcas de
/// cardinalidad del ER, aristas rectas): su área es cero, así que el relleno
/// no pinta nada y tiny_skia avisa en cada dibujo. Se les pone `fill:none`.
fn unfill_flat_paths(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(at) = rest.find("<path") {
        out.push_str(&rest[..at]);
        let Some(len) = rest[at..].find('>').map(|i| i + 1) else {
            break;
        };
        let tag = &rest[at..at + len];
        out.push_str(&match path_data(tag).filter(|d| is_flat(d)) {
            Some(_) => without_fill(tag),
            None => tag.to_owned(),
        });
        rest = &rest[at + len..];
    }
    out.push_str(rest);
    out
}

/// El atributo `d` de una etiqueta `<path …>`.
fn path_data(tag: &str) -> Option<&str> {
    let start = tag.find(" d=\"")? + 4;
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

/// Por debajo de esta extensión (en px), un relleno no se ve: merman dibuja
/// así los separadores del ER (rectángulos de 0,0001 px).
const FLAT: f64 = 0.01;

/// ¿Cada subtrazo es (casi) una recta horizontal o vertical? Solo con
/// comandos absolutos de pares de coordenadas (M, L, C); con otros no se
/// arriesga.
fn is_flat(d: &str) -> bool {
    if d.chars()
        .any(|c| c.is_ascii_alphabetic() && !matches!(c, 'M' | 'L' | 'C'))
    {
        return false;
    }
    let subpaths: Vec<&str> = d.split('M').filter(|s| !s.trim().is_empty()).collect();
    !subpaths.is_empty() && subpaths.iter().all(|s| is_flat_subpath(s))
}

fn is_flat_subpath(d: &str) -> bool {
    let numbers: Vec<f64> = d
        .split(|c: char| c.is_ascii_alphabetic() || c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    if numbers.len() < 4 || !numbers.len().is_multiple_of(2) {
        return false;
    }
    let extent = |offset: usize| {
        let (min, max) = numbers
            .iter()
            .skip(offset)
            .step_by(2)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(*v), hi.max(*v))
            });
        max - min
    };
    extent(0) < FLAT || extent(1) < FLAT
}

/// La etiqueta con el relleno anulado: en el estilo en línea si lo tiene; si
/// no, como atributo.
fn without_fill(tag: &str) -> String {
    if let Some(start) = tag.find("style=\"").map(|i| i + 7)
        && let Some(end) = tag[start..].find('"').map(|i| i + start)
    {
        let style: Vec<&str> = tag[start..end]
            .split(';')
            .filter(|decl| !decl.trim_start().starts_with("fill:"))
            .collect();
        let mut style = style.join(";");
        if !style.is_empty() && !style.ends_with(';') {
            style.push(';');
        }
        style.push_str("fill:none");
        return format!("{}{}{}", &tag[..start], style, &tag[end..]);
    }
    let close = if tag.ends_with("/>") {
        tag.len() - 2
    } else {
        tag.len() - 1
    };
    format!("{} style=\"fill:none\"{}", &tag[..close], &tag[close..])
}

/// `filter:drop-shadow(…)` → `filter:none`. merman la pone en el CSS de los
/// nodos y resvg la dibuja con un desenfoque por nodo: de 40 ms a 3 s por
/// raster en los diagramas grandes (TC-3).
fn without_shadows(svg: &str) -> String {
    const SHADOW: &str = "filter:drop-shadow(";
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(at) = rest.find(SHADOW) {
        let (before, after) = rest.split_at(at);
        // Hasta su `)`, saltando los paréntesis de dentro (un `rgba(…)`).
        let mut depth = 0usize;
        let close = after.char_indices().skip(SHADOW.len() - 1).find(|(_, c)| {
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ => {}
            }
            depth == 0
        });
        let Some((close, _)) = close else {
            break;
        };
        out.push_str(before);
        out.push_str("filter:none");
        rest = after.get(close + 1..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

/// Mide el texto con las caras de [`Fonts`], igual que dibujará resvg.
fn measurement_policy(fonts: &'static Fonts) -> Option<TextMeasurementPolicy> {
    let id = MeasurementProfileId::new("taskcraft.fontdb").ok()?;
    let identity = TextMeasurementProfileIdentity::new(id, format!("1:{}", fonts.family)).ok()?;
    let measurer = DeterministicTextMeasurer::default()
        .with_width_callback(move |text, style| fonts.width(text, style));
    Some(TextMeasurementPolicy::uniform(TextMeasurementProfile::new(
        identity, measurer,
    )))
}

/// Tema claro: el de Mermaid. Oscuro: roles a juego con el tema oscuro de la
/// app. En los dos, la familia con la que se mide y se dibuja.
fn host_theme(theme: Theme, family: &str) -> HostTheme {
    let base = HostTheme::new();
    let base = base
        .clone()
        .try_with_font_family(format!("{family}, sans-serif"))
        .unwrap_or(base);
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

/// Una cara de fuente cargada: datos y su índice en el archivo.
struct Face {
    data: Arc<[u8]>,
    index: u32,
}

/// Fuentes del sistema más las de egui embebidas, para que el texto se vea
/// aunque el sistema no tenga ninguna, y la familia con la que merman mide y
/// resvg dibuja: la primera conocida que exista. `sans-serif` apunta a ella.
struct Fonts {
    db: Arc<usvg::fontdb::Database>,
    family: String,
    regular: Option<Face>,
    bold: Option<Face>,
    /// Ancho en em de cada texto ya medido, por (negrita, texto).
    widths: Mutex<HashMap<(bool, String), f64>>,
}

impl Fonts {
    /// Ancho en píxeles de `text` con el tamaño y el peso de `style`.
    fn width(&self, text: &str, style: &TextStyle) -> f64 {
        let size = style.font_size.max(1.0);
        if text.is_empty() {
            return 0.0;
        }
        let bold = style.font_weight.as_deref().is_some_and(|w| {
            w.eq_ignore_ascii_case("bold")
                || w.eq_ignore_ascii_case("bolder")
                || w.parse::<u32>().is_ok_and(|n| n >= 600)
        });
        let key = (bold, text.to_owned());
        if let Ok(cache) = self.widths.lock()
            && let Some(em) = cache.get(&key)
        {
            return em * size;
        }
        let face = if bold {
            self.bold.as_ref().or(self.regular.as_ref())
        } else {
            self.regular.as_ref()
        };
        // Sin cara que medir, una aproximación generosa: nunca recorta.
        let em = face
            .and_then(|f| advance_em(f, text))
            .unwrap_or(text.chars().count() as f64 * 0.6);
        if let Ok(mut cache) = self.widths.lock() {
            if cache.len() > 50_000 {
                cache.clear();
            }
            cache.insert(key, em);
        }
        em * size
    }
}

/// Avance de `text` en em, con harfrust (el mismo shaper que usa usvg).
fn advance_em(face: &Face, text: &str) -> Option<f64> {
    let font = harfrust::FontRef::from_index(&face.data, face.index).ok()?;
    let data = harfrust::ShaperData::new(&font);
    let shaper = data.shaper(&font).build();
    let upem = shaper.units_per_em();
    if upem <= 0 {
        return None;
    }
    let mut buffer = harfrust::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let output = shaper.shape(buffer, harfrust::ShapeOptions::new());
    let advance: i64 = output
        .glyph_positions()
        .iter()
        .map(|p| i64::from(p.x_advance))
        .sum();
    Some(advance as f64 / f64::from(upem))
}

fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        db.load_font_data(epaint_default_fonts::UBUNTU_LIGHT.to_vec());
        db.load_font_data(epaint_default_fonts::HACK_REGULAR.to_vec());
        let has = |name: &str| {
            db.faces()
                .any(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)))
        };
        let family = [
            "Arial",
            "Liberation Sans",
            "DejaVu Sans",
            "Noto Sans",
            "Segoe UI",
            "Ubuntu",
        ]
        .into_iter()
        .find(|name| has(name))
        .unwrap_or("Ubuntu")
        .to_owned();
        db.set_sans_serif_family(family.as_str());
        let face = |weight: usvg::fontdb::Weight| {
            let query = usvg::fontdb::Query {
                families: &[usvg::fontdb::Family::Name(&family)],
                weight,
                ..usvg::fontdb::Query::default()
            };
            let id = db.query(&query)?;
            db.with_face_data(id, |data, index| Face {
                data: Arc::from(data),
                index,
            })
        };
        let regular = face(usvg::fontdb::Weight::NORMAL);
        let bold = face(usvg::fontdb::Weight::BOLD);
        Fonts {
            db: Arc::new(db),
            family,
            regular,
            bold,
            widths: Mutex::new(HashMap::new()),
        }
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

    /// TC-2: se maqueta una vez y se rasteriza a cada escala.
    #[test]
    fn one_drawing_rasterizes_at_any_scale() {
        let drawing = Drawing::new(FLOW, Theme::Dark).expect("dibujo");
        let (w, h) = drawing.size();
        assert!(w > 0.0 && h > 0.0);
        for scale in [0.5, 1.0, 2.0] {
            let img = drawing.rasterize(scale).expect("raster");
            assert_eq!(img.width, (w * scale).ceil() as u32);
            assert_eq!(img.height, (h * scale).ceil() as u32);
        }
    }

    #[test]
    fn raster_is_bounded() {
        let svg = render_svg(FLOW, Theme::Light).expect("svg");
        let img = rasterize(&svg, 1000.0).expect("raster");
        assert!(img.width <= MAX_SIDE && img.height <= MAX_SIDE);
    }

    /// Cuenta los avisos (`warn` o peor) de cualquier crate, por hilo: los
    /// demás tests del binario corren en paralelo y pueden avisar a la vez.
    struct Warnings;
    thread_local! {
        static WARNINGS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }
    impl log::Log for Warnings {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= log::Level::Warn
        }
        fn log(&self, r: &log::Record) {
            if self.enabled(r.metadata()) {
                WARNINGS.with(|w| w.set(w.get() + 1));
            }
        }
        fn flush(&self) {}
    }
    static LOGGER: Warnings = Warnings;

    const ER: &str = "erDiagram\n  project ||--o{ task : contiene\n  task ||--o| today_mark : hoy\n  project {\n    text id PK\n    text key UK\n  }\n  task {\n    text id PK\n    text project_id FK\n  }";
    const CLASSES: &str = "classDiagram\n  class Agent {\n    <<enumeration>>\n    +String name\n    +Vec~String~ capabilities\n  }\n  class Interface {\n    +Option~String~ token\n  }\n  Interface \"1\" *-- \"*\" Agent";

    /// TC-1: ni usvg ni tiny_skia avisan al dibujar (antes: transform:none,
    /// drop-shadow con rgba, separadores de 0,0001 px del ER…).
    #[test]
    fn rendering_produces_no_warnings() {
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Warn);
        for theme in [Theme::Light, Theme::Dark] {
            for source in [ER, CLASSES, FLOW, SEQ] {
                let before = WARNINGS.with(std::cell::Cell::get);
                Drawing::new(source, theme)
                    .and_then(|d| d.rasterize(1.0))
                    .expect("raster");
                let after = WARNINGS.with(std::cell::Cell::get);
                assert_eq!(before, after, "avisos al dibujar {source:?} ({theme:?})");
            }
        }
    }

    #[test]
    fn the_svg_is_cleaned_for_usvg() {
        let svg = clean_svg(
            r#"<style>.node rect{stroke:#6b8bb5;filter:drop-shadow(1px 2px 2px #b9b9b9);}</style><g style="opacity:1;transform:none"/><g style="transform:none;fill:red"/><path style="filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1))" d="M0,0 L10,10"/><text dominant-baseline="start">x</text><rect style="filter:drop-shadow(1px 1px 1px rgba(0,0,0,0.5));opacity:1"/>"#,
        );
        assert!(!svg.contains("transform:none"), "{svg}");
        assert!(!svg.contains("drop-shadow"), "sin sombras: {svg}");
        assert!(svg.contains("stroke:#6b8bb5;filter:none;}"), "{svg}");
        assert!(svg.contains("filter:none;opacity:1"), "{svg}");
        assert!(svg.contains(r#"dominant-baseline="auto""#), "{svg}");
        assert!(svg.contains("fill:red"), "lo demás queda igual");
    }

    #[test]
    fn flat_paths_lose_their_fill_and_the_rest_keep_it() {
        // Marca de cardinalidad del ER: dos verticales.
        assert!(is_flat("M9,0 L9,18 M15,0 L15,18"));
        // Separador de 0,0001 px.
        assert!(is_flat(
            "M-140.609425 -128.25 L-140.609325 -128.25 L-140.609325 171 L-140.609425 171"
        ));
        assert!(!is_flat("M0 0 L10 0 L10 10 L0 10"));
        assert!(!is_flat("M0,18 Q 18,0 36,18"), "con Q no se arriesga");
        assert!(!is_flat("m0 0 l10 0"), "relativos tampoco");
        let tag = r#"<path d="M9,0 L9,18" stroke-width="1"/>"#;
        assert_eq!(
            unfill_flat_paths(tag),
            r#"<path d="M9,0 L9,18" stroke-width="1" style="fill:none"/>"#
        );
        let styled = r#"<path d="M0 5 L9 5" style="stroke:#000;fill:#fff"/>"#;
        assert_eq!(
            unfill_flat_paths(styled),
            r#"<path d="M0 5 L9 5" style="stroke:#000;fill:none"/>"#
        );
        let normal = r##"<path d="M0 0 L10 0 L10 10" fill="#fff"/>"##;
        assert_eq!(unfill_flat_paths(normal), normal);
    }

    /// TC-1: merman mide con la fuente con la que dibuja resvg.
    #[test]
    fn text_is_measured_with_the_drawing_font() {
        let fonts = fonts();
        let regular = TextStyle {
            font_size: 16.0,
            ..TextStyle::default()
        };
        let bold = TextStyle {
            font_weight: Some("bold".into()),
            ..regular.clone()
        };
        let plain = fonts.width("Agent", &regular);
        assert!(plain > 0.0);
        assert!(
            fonts.width("Agent", &bold) >= plain,
            "la negrita no es más estrecha"
        );
        assert!(fonts.width("AgentAgent", &regular) > plain * 1.9);
        let svg = render_svg(CLASSES, Theme::Light).expect("svg");
        assert!(svg.contains(&fonts.family), "el SVG usa la familia medida");
    }
}
