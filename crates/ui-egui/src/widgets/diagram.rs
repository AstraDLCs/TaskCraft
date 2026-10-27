//! Diagramas Mermaid dibujados (RF-MOD-19, D21): la fuente se maqueta una vez
//! por tema con el crate `diagram` y cada escala pedida queda como textura.
//! Con [`DiagramCache::in_background`] (la app) maquetar y rasterizar van en
//! un hilo, y mientras llega la escala pedida se estira la más cercana que ya
//! haya: el zoom nunca congela la ventana (TC-3). Sin él (tests), todo es
//! síncrono. Si no se puede dibujar, se ve el aviso con la fuente.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use diagram::{DiagramError, Drawing, Image, MAX_SIDE, Theme};
use eframe::egui::{self, ColorImage, RichText, TextureHandle, TextureOptions, Ui, Vec2};
use i18n::{Lang, Localize, tr};

use crate::theme;
use crate::widgets::icons::{self, Icon};

/// Dibujos maquetados guardados; más que esto y se empieza de cero.
const MAX_DRAWINGS: usize = 32;
/// Memoria de las texturas guardadas (RGBA); al pasarla se liberan todas y
/// se rehacen solo las que se vean. Una sola llega a 4096² × 4 = 64 MiB.
const MAX_TEXTURE_BYTES: usize = 256 << 20;

/// Una fuente en un tema.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Source {
    hash: u64,
    dark: bool,
}

impl Source {
    fn of(ui: &Ui, source: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        Self {
            hash: hasher.finish(),
            dark: ui.visuals().dark_mode,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    source: Source,
    /// Escala del raster en cuartos (4 = 1×).
    quarters: u32,
}

/// Escala de un trabajo que solo maqueta, para saber el tamaño natural.
const LAYOUT_ONLY: u32 = 0;

impl Key {
    /// `zoom` a la densidad de la pantalla. Con el tamaño `natural`, sin pasar
    /// de la escala a la que el raster llega a [`MAX_SIDE`]: más allá la imagen
    /// sería la misma y se rasterizaría y guardaría dos veces.
    fn new(ui: &Ui, source: Source, zoom: f32, natural: Option<Vec2>) -> Self {
        let scale = (ui.ctx().pixels_per_point() * zoom).clamp(0.25, 8.0);
        let mut quarters = (scale * 4.0).round() as u32;
        if let Some(natural) = natural {
            let longest = natural.x.max(natural.y).max(1.0);
            let cap = (MAX_SIDE as f32 / longest * 4.0).floor() as u32;
            quarters = quarters.min(cap.max(1));
        }
        Self { source, quarters }
    }

    fn layout(source: Source) -> Self {
        Self {
            source,
            quarters: LAYOUT_ONLY,
        }
    }

    fn scale(self) -> f32 {
        self.quarters as f32 / 4.0
    }
}

/// La textura ya hecha de la misma fuente más cercana a `want`: mientras
/// llega la pedida, mejor una algo borrosa que un hueco.
fn nearest(made: impl Iterator<Item = Key>, want: Key) -> Option<Key> {
    made.filter(|k| k.source == want.source)
        .min_by_key(|k| (k.quarters.abs_diff(want.quarters), u32::MAX - k.quarters))
}

/// Dibujar `source` a la escala de `key`.
struct Job {
    key: Key,
    source: String,
}

/// Lo dibujado: tamaño natural en puntos e imagen ya lista para subirla
/// como textura (`None` si solo se maquetó), o por qué no se pudo.
struct Drawn {
    natural: Result<Vec2, DiagramError>,
    image: Option<Result<ColorImage, DiagramError>>,
}

/// Píxeles de resvg → imagen de egui; en el hilo de dibujo, que con 16 M de
/// píxeles se nota.
fn color_image(image: Image) -> ColorImage {
    ColorImage::from_rgba_premultiplied([image.width as usize, image.height as usize], &image.rgba)
}

/// Respuesta a un [`Job`]; `drawn` es `None` si se descartó porque llegó otra
/// petición de la misma fuente.
struct Done {
    key: Key,
    drawn: Option<Drawn>,
}

/// Dibujos maquetados por fuente y tema.
#[derive(Default)]
struct Drawings(HashMap<Source, Result<Drawing, DiagramError>>);

impl Drawings {
    fn draw(&mut self, job: &Job) -> Drawn {
        let id = job.key.source;
        if !self.0.contains_key(&id) && self.0.len() >= MAX_DRAWINGS {
            self.0.clear();
        }
        let theme = if id.dark { Theme::Dark } else { Theme::Light };
        match self
            .0
            .entry(id)
            .or_insert_with(|| Drawing::new(&job.source, theme))
        {
            Ok(drawing) => {
                let (w, h) = drawing.size();
                Drawn {
                    natural: Ok(Vec2::new(w, h)),
                    image: (job.key.quarters != LAYOUT_ONLY)
                        .then(|| drawing.rasterize(job.key.scale()).map(color_image)),
                }
            }
            Err(e) => Drawn {
                natural: Err(e.clone()),
                image: Some(Err(e.clone())),
            },
        }
    }
}

/// De cada fuente, solo la última petición: la rueda pide varios escalones
/// seguidos y solo importa donde se quedó. Devuelve (a dibujar, descartadas).
fn latest_per_source(batch: impl IntoIterator<Item = Job>) -> (Vec<Job>, Vec<Key>) {
    let mut keep: Vec<Job> = Vec::new();
    let mut stale = Vec::new();
    for job in batch {
        match keep.iter_mut().find(|j| j.key.source == job.key.source) {
            Some(old) => stale.push(std::mem::replace(old, job).key),
            None => keep.push(job),
        }
    }
    (keep, stale)
}

/// Hilo que maqueta y rasteriza; despierta la ventana con cada resultado.
struct Worker {
    jobs: Sender<Job>,
    done: Receiver<Done>,
}

impl Worker {
    fn spawn(ctx: egui::Context) -> Option<Self> {
        let (jobs, inbox) = channel::<Job>();
        let (outbox, done) = channel();
        std::thread::Builder::new()
            .name("taskcraft-diagrams".into())
            .spawn(move || {
                let mut drawings = Drawings::default();
                while let Ok(first) = inbox.recv() {
                    let (keep, stale) =
                        latest_per_source(std::iter::once(first).chain(inbox.try_iter()));
                    let skipped = stale.into_iter().map(|key| Done { key, drawn: None });
                    let drawn = keep.into_iter().map(|job| Done {
                        key: job.key,
                        drawn: Some(drawings.draw(&job)),
                    });
                    for done in skipped.chain(drawn) {
                        if outbox.send(done).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                    }
                }
            })
            .ok()?;
        Some(Self { jobs, done })
    }
}

type Entry = Result<(TextureHandle, Vec2), DiagramError>;

#[derive(Default)]
pub struct DiagramCache {
    /// Dibujar en un hilo ([`Self::in_background`]); si no, en la llamada.
    background: bool,
    worker: Option<Worker>,
    /// Los dibujos, cuando no hay hilo.
    drawings: Drawings,
    naturals: HashMap<Source, Result<Vec2, DiagramError>>,
    textures: HashMap<Key, Result<TextureHandle, DiagramError>>,
    texture_bytes: usize,
    /// Pedidas al hilo y aún sin respuesta.
    pending: HashSet<Key>,
}

impl DiagramCache {
    /// Maquetar y rasterizar en un hilo aparte, que se crea al primer dibujo.
    pub fn in_background(&mut self) {
        self.background = true;
    }

    /// Texturas (o errores de raster) guardadas.
    #[cfg(test)]
    pub fn cached(&self) -> usize {
        self.textures.len()
    }

    /// Tamaño natural de `source` en puntos; `None` mientras se maqueta.
    pub fn natural(&mut self, ui: &Ui, source: &str) -> Option<Result<Vec2, DiagramError>> {
        let id = Source::of(ui, source);
        self.receive(ui.ctx());
        if !self.naturals.contains_key(&id) {
            self.request(ui.ctx(), Key::layout(id), source);
        }
        self.naturals.get(&id).cloned()
    }

    /// Textura de `source` para dibujarla a `zoom` (1 = tamaño natural) y su
    /// tamaño natural en puntos, rasterizada a la densidad de la pantalla.
    /// Mientras llega, la más cercana ya hecha; `None` si aún no hay ninguna.
    pub fn get(&mut self, ui: &Ui, source: &str, zoom: f32) -> Option<Entry> {
        let id = Source::of(ui, source);
        self.receive(ui.ctx());
        let known = self
            .naturals
            .get(&id)
            .and_then(|n| n.as_ref().ok())
            .copied();
        let key = Key::new(ui, id, zoom, known);
        if !self.textures.contains_key(&key) {
            self.request(ui.ctx(), key, source);
        }
        let natural = match self.naturals.get(&id)? {
            Ok(natural) => *natural,
            Err(e) => return Some(Err(e.clone())),
        };
        let shown = match self.textures.get(&key) {
            Some(texture) => texture,
            None => {
                let made = self
                    .textures
                    .iter()
                    .filter(|(_, t)| t.is_ok())
                    .map(|(k, _)| *k);
                self.textures.get(&nearest(made, key)?)?
            }
        };
        Some(shown.clone().map(|t| (t, natural)))
    }

    fn request(&mut self, ctx: &egui::Context, key: Key, source: &str) {
        if !self.pending.insert(key) {
            return;
        }
        let job = Job {
            key,
            source: source.to_owned(),
        };
        if self.background && self.worker.is_none() {
            self.worker = Worker::spawn(ctx.clone());
            // Sin hilo (no se pudo crear), se sigue en la llamada.
            self.background = self.worker.is_some();
        }
        let job = match &self.worker {
            Some(worker) => match worker.jobs.send(job) {
                Ok(()) => return,
                // El hilo terminó: lo que queda, en la llamada, y lo que
                // tenía pedido se vuelve a pedir.
                Err(unsent) => {
                    self.worker = None;
                    self.background = false;
                    self.pending.clear();
                    self.pending.insert(key);
                    unsent.0
                }
            },
            None => job,
        };
        let drawn = self.drawings.draw(&job);
        self.store(
            ctx,
            Done {
                key,
                drawn: Some(drawn),
            },
        );
    }

    /// Recoge lo que terminó el hilo.
    fn receive(&mut self, ctx: &egui::Context) {
        loop {
            let Some(worker) = &self.worker else { return };
            match worker.done.try_recv() {
                Ok(done) => self.store(ctx, done),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    // Lo pendiente se vuelve a pedir, ya sin hilo.
                    self.worker = None;
                    self.background = false;
                    self.pending.clear();
                    return;
                }
            }
        }
    }

    fn store(&mut self, ctx: &egui::Context, done: Done) {
        self.pending.remove(&done.key);
        let Some(drawn) = done.drawn else {
            return;
        };
        let key = done.key;
        if !self.naturals.contains_key(&key.source) && self.naturals.len() >= MAX_DRAWINGS {
            self.naturals.clear();
            self.textures.clear();
            self.texture_bytes = 0;
        }
        self.naturals.insert(key.source, drawn.natural);
        let Some(image) = drawn.image else {
            return;
        };
        let bytes = image.as_ref().map_or(0, |i| i.pixels.len() * 4);
        if self.texture_bytes + bytes > MAX_TEXTURE_BYTES {
            self.textures.clear();
            self.texture_bytes = 0;
        }
        self.texture_bytes += bytes;
        let texture = image.map(|image| {
            ctx.load_texture(
                format!("diagram-{:x}-{}", key.source.hash, key.quarters),
                image,
                TextureOptions::LINEAR,
            )
        });
        self.textures.insert(key, texture);
    }
}

/// El diagrama a su tamaño natural, reducido si no cabe a lo ancho. Devuelve
/// la respuesta de la imagen (clic), o `None` si se mostró el error o aún se
/// está dibujando.
pub fn fitted(
    ui: &mut Ui,
    cache: &mut DiagramCache,
    lang: Lang,
    source: &str,
) -> Option<egui::Response> {
    match cache.get(ui, source, 1.0) {
        Some(Ok((texture, natural))) => {
            let width = natural.x.min(ui.available_width()).max(1.0);
            let size = natural * (width / natural.x.max(1.0));
            Some(ui.add(egui::Image::new((texture.id(), size)).sense(egui::Sense::click())))
        }
        Some(Err(e)) => {
            error(ui, lang, &e, source);
            None
        }
        None => {
            ui.spinner();
            None
        }
    }
}

/// Límites del zoom del visor (1 = tamaño natural).
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 4.0;
/// Escalones de raster: el zoom se dibuja con el escalón igual o mayor más
/// cercano, nítido sin volver a rasterizar en cada movimiento de la rueda.
const RASTER_STEPS: [f32; 8] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0];
/// Factor de los botones − y +.
const ZOOM_STEP: f32 = 1.25;
/// Puntos de rueda que multiplican el zoom por e (una muesca ≈ ×1,28). La
/// rueda vertical amplía (TC-2): en Linux (winit 0.30) el touchpad no envía
/// el pellizco, así que el deslizamiento vertical de dos dedos hace de zoom.
const WHEEL_ZOOM: f32 = 200.0;

/// Zoom y desplazamiento del visor de un diagrama; `None` en `zoom` = aún no
/// se ajustó a la ventana.
#[derive(Clone, Copy, Debug, Default)]
struct View {
    zoom: Option<f32>,
    pan: Vec2,
}

/// El zoom que hace caber `natural` en `area`, sin pasar del tamaño natural.
fn fit_zoom(natural: Vec2, area: Vec2) -> f32 {
    let x = area.x / natural.x.max(1.0);
    let y = area.y / natural.y.max(1.0);
    x.min(y).clamp(MIN_ZOOM, 1.0)
}

/// Desplazamiento que centra la imagen de tamaño `image` en `area`.
fn centered(image: Vec2, area: Vec2) -> Vec2 {
    (area - image) / 2.0
}

/// Multiplica `zoom` por `factor` (dentro de los límites) dejando quieto lo
/// que está bajo `anchor` (relativo al lienzo). Devuelve (zoom, pan).
fn zoom_around(zoom: f32, pan: Vec2, factor: f32, anchor: Vec2) -> (f32, Vec2) {
    let next = (zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    (next, anchor - (anchor - pan) * (next / zoom))
}

/// Mantiene a la vista al menos un margen de la imagen al desplazarla; si
/// cabe entera en un eje, la centra en ese eje.
fn clamp_pan(pan: Vec2, image: Vec2, area: Vec2) -> Vec2 {
    let axis = |pan: f32, image: f32, area: f32| {
        if image <= area {
            (area - image) / 2.0
        } else {
            pan.clamp(area - image - 40.0, 40.0)
        }
    };
    Vec2::new(axis(pan.x, image.x, area.x), axis(pan.y, image.y, area.y))
}

/// Aplica los gestos sobre el lienzo y devuelve (zoom, pan). `pinch` es el
/// `zoom_delta` de egui (pellizco y Ctrl+rueda); `scroll`, la rueda, que egui
/// pasa a horizontal con Shift. Si la rueda no amplía, no se toca.
fn gesture(
    (zoom, pan): (f32, Vec2),
    pinch: f32,
    scroll: Vec2,
    anchor: Vec2,
    wheel_zooms: bool,
) -> (f32, Vec2) {
    let (mut zoom, mut pan) = (zoom, pan);
    if (pinch - 1.0).abs() > f32::EPSILON {
        (zoom, pan) = zoom_around(zoom, pan, pinch, anchor);
    }
    if wheel_zooms {
        if scroll.y != 0.0 {
            (zoom, pan) = zoom_around(zoom, pan, (scroll.y / WHEEL_ZOOM).exp(), anchor);
        }
        pan.x += scroll.x;
    }
    (zoom, pan)
}

/// Visor con zoom (TC-1, TC-2): barra con −, %, +, 100 % y Ajustar;
/// Ctrl+rueda y el pellizco amplían alrededor del cursor; arrastrar desplaza;
/// doble clic ajusta. Con `wheel_zooms`, además la rueda (o dos dedos en
/// vertical) amplía y Shift+rueda desplaza; sin él, la rueda sigue siendo de
/// la página que lo contiene (vista previa del editor). Ocupa el ancho
/// disponible y `height` de alto.
pub fn viewer(
    ui: &mut Ui,
    cache: &mut DiagramCache,
    lang: Lang,
    id_salt: impl Hash + std::fmt::Debug,
    source: &str,
    height: f32,
    wheel_zooms: bool,
) {
    let id = ui.make_persistent_id(("diagram_viewer", id_salt));
    let mut view: View = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let natural = match cache.natural(ui, source) {
        Some(Ok(natural)) => natural,
        Some(Err(e)) => {
            error(ui, lang, &e, source);
            return;
        }
        None => {
            // Aún se maqueta (en el hilo de dibujo): el hueco, con el indicador.
            let size = Vec2::new(ui.available_width().max(1.0), height.max(80.0));
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.put(rect, egui::Spinner::new());
            return;
        }
    };

    let mut fit = false;
    let mut zoom_by: Option<f32> = None;
    let mut reset = false;
    ui.horizontal_wrapped(|ui| {
        let zoom = view.zoom.unwrap_or(1.0);
        if ui
            .add_enabled(zoom > MIN_ZOOM, egui::Button::new("−"))
            .on_hover_text(i18n::t(lang, "ui-zoom-out"))
            .clicked()
        {
            zoom_by = Some(1.0 / ZOOM_STEP);
        }
        ui.label(RichText::new(format!("{:.0} %", zoom * 100.0)).monospace());
        if ui
            .add_enabled(zoom < MAX_ZOOM, egui::Button::new("+"))
            .on_hover_text(i18n::t(lang, "ui-zoom-in"))
            .clicked()
        {
            zoom_by = Some(ZOOM_STEP);
        }
        reset = ui.button("100 %").clicked();
        fit = ui.button(i18n::t(lang, "ui-zoom-fit")).clicked();
        let hint = if wheel_zooms {
            "ui-zoom-hint"
        } else {
            "ui-zoom-hint-ctrl"
        };
        ui.weak(i18n::t(lang, hint));
    });

    let size = Vec2::new(ui.available_width().max(1.0), height.max(80.0));
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let area = rect.size();
    if view.zoom.is_none() || fit || response.double_clicked() {
        let zoom = fit_zoom(natural, area);
        view = View {
            zoom: Some(zoom),
            pan: centered(natural * zoom, area),
        };
    }
    let mut zoom = view.zoom.unwrap_or(1.0);

    // Ampliar alrededor de un punto: lo que está bajo él se queda ahí.
    let middle = area / 2.0;
    if reset {
        (zoom, view.pan) = zoom_around(zoom, view.pan, 1.0 / zoom, middle);
    }
    if let Some(factor) = zoom_by {
        (zoom, view.pan) = zoom_around(zoom, view.pan, factor, middle);
    }
    if response.hovered() {
        let (pinch, scroll, pointer) =
            ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.pointer.hover_pos()));
        let anchor = pointer.map_or(middle, |p| p - rect.min);
        (zoom, view.pan) = gesture((zoom, view.pan), pinch, scroll, anchor, wheel_zooms);
        if wheel_zooms && scroll != Vec2::ZERO {
            // La rueda es del diagrama, no de la página que lo contiene.
            ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
        }
    }
    if response.dragged() {
        view.pan += response.drag_delta();
    }
    view.pan = clamp_pan(view.pan, natural * zoom, area);
    view.zoom = Some(zoom);
    ui.data_mut(|d| d.insert_temp(id, view));

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let raster = RASTER_STEPS
        .iter()
        .copied()
        .find(|s| *s >= zoom)
        .unwrap_or(MAX_ZOOM);
    match cache.get(ui, source, raster) {
        Some(Ok((texture, _))) => {
            let image = egui::Rect::from_min_size(rect.min + view.pan, natural * zoom);
            painter.image(
                texture.id(),
                image,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        Some(Err(e)) => {
            error(ui, lang, &e, source);
        }
        None => {
            ui.put(rect, egui::Spinner::new());
        }
    }
    let cursor = if response.dragged() {
        egui::CursorIcon::Grabbing
    } else {
        egui::CursorIcon::Grab
    };
    response.on_hover_cursor(cursor);
}

/// Aviso de que no se puede dibujar, con el motivo y la fuente.
pub fn error(ui: &mut Ui, lang: Lang, e: &DiagramError, source: &str) {
    egui::Frame::group(ui.style())
        .stroke(egui::Stroke::new(1.0, theme::ORANGE))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                icons::show(ui, Icon::Warning, theme::ORANGE);
                ui.label(RichText::new(tr(lang, &e.msg())).color(theme::ORANGE));
            });
            if !source.trim().is_empty() {
                ui.add(egui::Label::new(RichText::new(source).monospace().weak()).wrap());
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting_never_enlarges_and_keeps_the_image_in_view() {
        let natural = Vec2::new(2000.0, 1000.0);
        assert_eq!(fit_zoom(natural, Vec2::new(1000.0, 1000.0)), 0.5);
        assert_eq!(
            fit_zoom(Vec2::new(100.0, 50.0), Vec2::new(800.0, 600.0)),
            1.0
        );
        assert_eq!(
            centered(Vec2::new(100.0, 50.0), Vec2::new(300.0, 150.0)),
            Vec2::new(100.0, 50.0)
        );
        let area = Vec2::new(500.0, 400.0);
        // Más pequeña que el área: centrada aunque se arrastre.
        assert_eq!(
            clamp_pan(Vec2::new(-999.0, 999.0), Vec2::new(100.0, 100.0), area),
            Vec2::new(200.0, 150.0)
        );
        // Más grande: se puede mover, pero siempre queda algo a la vista.
        let pan = clamp_pan(Vec2::new(-5000.0, 5000.0), Vec2::new(2000.0, 2000.0), area);
        assert_eq!(pan, Vec2::new(500.0 - 2000.0 - 40.0, 40.0));
    }

    /// TC-2: al ampliar, el punto del diagrama bajo el cursor no se mueve.
    #[test]
    fn zooming_keeps_the_point_under_the_cursor() {
        let (zoom, pan) = (0.5, Vec2::new(30.0, 20.0));
        let anchor = Vec2::new(200.0, 150.0);
        let under = (anchor - pan) / zoom;
        let (next, moved) = zoom_around(zoom, pan, 2.0, anchor);
        assert_eq!(next, 1.0);
        assert_eq!((anchor - moved) / next, under);
        // Con los límites, no pasa de MAX_ZOOM ni baja de MIN_ZOOM.
        assert_eq!(zoom_around(3.0, pan, 10.0, anchor).0, MAX_ZOOM);
        assert_eq!(zoom_around(0.2, pan, 0.01, anchor).0, MIN_ZOOM);
    }

    fn key(hash: u64, quarters: u32) -> Key {
        Key {
            source: Source { hash, dark: true },
            quarters,
        }
    }

    fn job(hash: u64, quarters: u32) -> Job {
        Job {
            key: key(hash, quarters),
            source: "flowchart LR\n  A --> B".into(),
        }
    }

    /// TC-3: de varias peticiones seguidas de una fuente solo se dibuja la
    /// última; las de otras fuentes se respetan.
    #[test]
    fn only_the_latest_request_per_source_is_drawn() {
        let (keep, stale) = latest_per_source([job(1, 4), job(2, 4), job(1, 8), job(1, 12)]);
        let kept: Vec<Key> = keep.iter().map(|j| j.key).collect();
        assert_eq!(kept, vec![key(1, 12), key(2, 4)]);
        assert_eq!(stale, vec![key(1, 4), key(1, 8)]);
    }

    /// TC-3: mientras llega la escala pedida se usa la más cercana de la
    /// misma fuente (a igual distancia, la más nítida); nunca la de otra.
    #[test]
    fn the_nearest_texture_stands_in_while_the_sharp_one_arrives() {
        let made = [key(1, 4), key(1, 8), key(1, 16), key(2, 12)];
        assert_eq!(nearest(made.into_iter(), key(1, 12)), Some(key(1, 16)));
        assert_eq!(nearest(made.into_iter(), key(1, 6)), Some(key(1, 8)));
        assert_eq!(nearest(made.into_iter(), key(3, 4)), None);
    }

    /// TC-3: el hilo maqueta y rasteriza, y descarta lo que se pidió de más.
    #[test]
    fn the_worker_draws_off_the_ui_thread() {
        let worker = Worker::spawn(egui::Context::default()).expect("hilo");
        for quarters in [4, 8] {
            worker.jobs.send(job(7, quarters)).expect("enviar");
        }
        let wait = std::time::Duration::from_secs(30);
        let mut drawn = None;
        while drawn.is_none() {
            let done = worker.done.recv_timeout(wait).expect("respuesta");
            if done.key == key(7, 8) {
                drawn = done.drawn;
            } else {
                // La primera: dibujada si el hilo la tomó sola, o descartada.
                assert_eq!(done.key, key(7, 4));
            }
        }
        let drawn = drawn.expect("dibujada");
        let natural = drawn.natural.expect("tamaño");
        let image = drawn.image.expect("rasterizada").expect("imagen");
        assert_eq!(image.size[0], (natural.x * 2.0).ceil() as usize);
    }

    /// Un fotograma de `ctx` con un `Ui`.
    fn frame(ctx: &egui::Context, mut f: impl FnMut(&mut Ui)) {
        // Sin pintor: las texturas subidas se descartan.
        ctx.run_ui(egui::RawInput::default(), |ui| f(ui))
            .textures_delta
            .clear();
    }

    /// Repite fotogramas hasta que `f` devuelve algo (el hilo tarda).
    fn until<T>(ctx: &egui::Context, mut f: impl FnMut(&mut Ui) -> Option<T>) -> T {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let mut got = None;
            frame(ctx, |ui| got = f(ui));
            if let Some(got) = got {
                return got;
            }
            assert!(std::time::Instant::now() < deadline, "el hilo no respondió");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// TC-3: con el hilo, al principio no hay nada (indicador de carga);
    /// luego llega el tamaño sin rasterizar, y al pedir otra escala se estira
    /// la que ya hay hasta que llega la nítida.
    #[test]
    fn the_cache_draws_in_the_background_and_stands_in_meanwhile() {
        let ctx = egui::Context::default();
        let mut cache = DiagramCache::default();
        cache.in_background();
        let source = "flowchart LR\n  A[Inicio] --> B[Fin]";
        frame(&ctx, |ui| {
            assert!(cache.natural(ui, source).is_none(), "aún nada")
        });
        let natural = until(&ctx, |ui| cache.natural(ui, source)).expect("tamaño");
        assert_eq!(cache.cached(), 0, "el tamaño no exige rasterizar");
        let small = until(&ctx, |ui| cache.get(ui, source, 1.0))
            .expect("1×")
            .0
            .size();
        let mut stood_in = None;
        frame(&ctx, |ui| stood_in = cache.get(ui, source, 2.0));
        let stood_in = stood_in.expect("reserva").expect("textura").0.size();
        assert_eq!(stood_in, small, "mientras tanto, la de 1× estirada");
        let sharp = until(&ctx, |ui| {
            cache
                .get(ui, source, 2.0)
                .and_then(Result::ok)
                .filter(|(t, _)| t.size() != small)
        });
        assert_eq!(sharp.1, natural);
        assert_eq!(sharp.0.size()[0], (natural.x * 2.0).ceil() as usize);
    }

    /// TC-3: pasado el lado máximo del raster, las escalas mayores son la
    /// misma imagen y comparten clave.
    #[test]
    fn scales_beyond_the_largest_raster_share_a_key() {
        let ctx = egui::Context::default();
        let id = Source {
            hash: 1,
            dark: true,
        };
        let natural = Some(Vec2::new(MAX_SIDE as f32 / 2.0, 100.0));
        frame(&ctx, |ui| {
            let two = Key::new(ui, id, 2.0, natural);
            assert_eq!(two.quarters, 8);
            assert_eq!(Key::new(ui, id, 3.0, natural), two);
            assert_eq!(
                Key::new(ui, id, 4.0, None).quarters,
                16,
                "sin tamaño, sin tope"
            );
        });
    }

    /// TC-2: la rueda amplía, Shift+rueda (horizontal) desplaza sin ampliar,
    /// y sin `wheel_zooms` la rueda no toca el visor pero el pellizco sí.
    #[test]
    fn gestures_zoom_or_pan_as_configured() {
        let start = (1.0, Vec2::new(10.0, 10.0));
        let anchor = Vec2::new(100.0, 100.0);
        let (zoom, _) = gesture(start, 1.0, Vec2::new(0.0, 120.0), anchor, true);
        assert!(zoom > 1.0, "la rueda amplía");
        let (zoom, pan) = gesture(start, 1.0, Vec2::new(-50.0, 0.0), anchor, true);
        assert_eq!(
            (zoom, pan),
            (1.0, Vec2::new(-40.0, 10.0)),
            "Shift+rueda mueve"
        );
        assert_eq!(
            gesture(start, 1.0, Vec2::new(-50.0, 120.0), anchor, false),
            start,
            "en la vista previa la rueda es de la página"
        );
        assert_eq!(gesture(start, 2.0, Vec2::ZERO, anchor, false).0, 2.0);
    }
}
