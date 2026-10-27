//! Barra lateral: proyectos desplegables con sus tareas, ciclos o sprints,
//! épicas, hitos y vistas guardadas; debajo, las secciones del workspace.
//! Plegada, queda el rail de iconos.

use eframe::egui::{self, collapsing_header::CollapsingState};
use i18n::{Msg, t, tr};
use model::{EpicKind, EpicStatus, Project, ProjectStatus};

use super::{TaskCraftApp, view_icon};
use crate::data::Scope;
use crate::layout;
use crate::state::{CycleForm, Dialog, GoTo, Intent, ProjectForm, ViewKind};
use crate::theme;
use crate::views::cycles::state_color;
use crate::widgets::date_picker::today;
use crate::widgets::icons::{self, Icon};
use crate::widgets::text::fitted;

/// Lo que se eligió en la barra y no es un comando: se aplica al terminar
/// de dibujarla, porque mientras tanto la réplica está prestada.
enum Pick {
    View(ViewKind),
    Saved(model::SavedView),
    Dialog(Dialog),
}

/// Lo que va delante del texto de una fila.
#[derive(Clone, Copy)]
enum Lead {
    None,
    Icon(Icon, egui::Color32),
    Dot(egui::Color32),
}

/// Fila de la barra a todo el ancho: icono, texto recortado con «…» y
/// contador a la derecha. El texto entero sale al pasar por encima si no cabe.
fn row(ui: &mut egui::Ui, lead: Lead, text: &str, count: &str, selected: bool) -> egui::Response {
    let height = ui.spacing().interact_size.y;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, text));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let visuals = ui.visuals();
    let (fill, color) = if selected {
        (visuals.selection.bg_fill, visuals.selection.stroke.color)
    } else if response.hovered() {
        (visuals.widgets.hovered.weak_bg_fill, visuals.text_color())
    } else {
        (egui::Color32::TRANSPARENT, visuals.text_color())
    };
    let weak = if selected {
        color
    } else {
        visuals.weak_text_color()
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, fill);
    let mut x = rect.left() + 4.0;
    let mid = rect.center().y;
    match lead {
        Lead::None => {}
        Lead::Icon(icon, tint) => {
            let size = 14.0;
            let at = egui::Rect::from_center_size(
                egui::pos2(x + size / 2.0, mid),
                egui::vec2(size, size),
            );
            icons::paint(painter, at, icon, if selected { color } else { tint });
            x += size + 6.0;
        }
        Lead::Dot(tint) => {
            painter.circle_filled(egui::pos2(x + 4.0, mid), 4.0, tint);
            x += 14.0;
        }
    }
    let font = egui::TextStyle::Body.resolve(ui.style());
    let mut right = rect.right() - 4.0;
    if !count.is_empty() {
        let small = egui::TextStyle::Small.resolve(ui.style());
        let galley = painter.layout_no_wrap(count.to_owned(), small, weak);
        right -= galley.size().x;
        painter.galley(egui::pos2(right, mid - galley.size().y / 2.0), galley, weak);
        right -= 6.0;
    }
    let galley = fitted(ui, text, font, color, right - x);
    let elided = galley.elided;
    painter.galley(egui::pos2(x, mid - galley.size().y / 2.0), galley, color);
    if elided {
        response.on_hover_text(text)
    } else {
        response
    }
}

/// Cabecera desplegable con sus filas. `plus`: botón «+» a la derecha.
fn section(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    open: bool,
    head: (Lead, &str, &str),
    plus: Option<&str>,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let id = ui.make_persistent_id(id);
    let (mut added, mut toggle) = (false, false);
    CollapsingState::load_with_default_open(ui.ctx(), id, open)
        .show_header(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(tip) = plus {
                    added = icons::button(ui, Icon::Plus, true, tip).clicked();
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let (lead, text, count) = head;
                    toggle = row(ui, lead, text, count, false).clicked();
                });
            });
        })
        .body(body);
    // Después de `body`, que guarda el estado con el que empezó el frame.
    if toggle {
        let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, open);
        state.toggle(ui);
        state.store(ui.ctx());
    }
    added
}

fn progress((done, total): (usize, usize)) -> String {
    if total == 0 {
        String::new()
    } else {
        format!("{done}/{total}")
    }
}

impl TaskCraftApp {
    /// Proyectos y secciones. `docked`: fija a la izquierda (si no, abierta
    /// encima de la vista, y se cierra al elegir algo).
    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui, docked: bool, out: &mut Vec<Intent>) {
        egui::ScrollArea::vertical()
            .id_salt("sidebar")
            .show(ui, |ui| self.sidebar_contents(ui, docked, out));
    }

    fn sidebar_contents(&mut self, ui: &mut egui::Ui, docked: bool, out: &mut Vec<Intent>) {
        let lang = self.st.lang;
        let mut pick = None;
        ui.horizontal(|ui| {
            ui.heading(t(lang, "ui-projects"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if docked
                    && icons::button(ui, Icon::Left, true, &t(lang, "ui-sidebar-collapse"))
                        .clicked()
                {
                    self.st.panels.sidebar_collapsed = true;
                }
                if icons::button(ui, Icon::Plus, true, &t(lang, "ui-new-project")).clicked() {
                    pick = Some(Pick::Dialog(Dialog::Project(ProjectForm::default())));
                }
            });
        });
        self.reveal_scope(ui);
        let in_tasks = ViewKind::TASK_VIEWS.contains(&self.st.view);
        let (active, archived): (Vec<_>, Vec<_>) = self
            .snap
            .projects
            .iter()
            .partition(|p| p.status == ProjectStatus::Active);
        let total: usize = active
            .iter()
            .filter_map(|p| self.snap.open_counts.get(&p.id))
            .sum();
        let all = in_tasks && self.st.scope == Scope::All && self.st.filter.cycle.is_none();
        if row(
            ui,
            Lead::Icon(Icon::List, theme::GRAY),
            &t(lang, "ui-all-projects"),
            &total.to_string(),
            all,
        )
        .clicked()
        {
            out.push(Intent::GoTo(GoTo::project(Scope::All)));
        }
        for p in &active {
            self.project_node(ui, p, in_tasks, &mut pick, out);
        }
        if active.is_empty() {
            ui.weak(t(lang, "ui-no-projects"));
        }
        if !archived.is_empty() {
            let head = tr(
                lang,
                &Msg::new("ui-archived-count").arg("count", archived.len()),
            );
            section(
                ui,
                "sb-archived",
                false,
                (Lead::None, &head, ""),
                None,
                |ui| {
                    for p in &archived {
                        let selected = in_tasks && self.st.scope == Scope::Project(p.id);
                        let name = format!("{} · {}", p.key, p.name);
                        if row(ui, Lead::None, &name, "", selected).clicked() {
                            out.push(Intent::GoTo(GoTo::project(Scope::Project(p.id))));
                        }
                    }
                },
            );
        }
        let shared: Vec<_> = self
            .snap
            .views
            .iter()
            .filter(|v| v.project_id.is_none())
            .collect();
        if !shared.is_empty() {
            ui.separator();
            let head = t(lang, "ui-saved-views");
            section(
                ui,
                "sb-views",
                true,
                (
                    Lead::Icon(Icon::Bookmark, theme::GRAY),
                    &head,
                    &shared.len().to_string(),
                ),
                None,
                |ui| self.view_rows(ui, &shared, &mut pick),
            );
        }
        ui.separator();
        let review = self.snap.review.len();
        for (view, icon, count) in self.nav_items() {
            let color = if view == ViewKind::Review && review > 0 {
                theme::VIOLET
            } else {
                ui.visuals().text_color()
            };
            let r = row(
                ui,
                Lead::Icon(icon, color),
                &t(lang, view.label_key()),
                &count,
                self.st.view == view,
            );
            if r.clicked() {
                pick = Some(Pick::View(view));
            }
        }
        match pick {
            Some(Pick::View(view)) => {
                self.st.view = view;
                self.st.panels.sidebar_peek = false;
            }
            Some(Pick::Saved(v)) => {
                self.st.panels.sidebar_peek = false;
                self.apply_view(&v);
            }
            Some(Pick::Dialog(d)) => {
                self.st.panels.sidebar_peek = false;
                self.st.dialog = Some(d);
            }
            None => {}
        }
    }

    /// Cuando cambia el proyecto elegido, despliega su nodo.
    fn reveal_scope(&self, ui: &egui::Ui) {
        let memo = egui::Id::unique("taskcraft-sidebar-scope");
        let last: Option<Scope> = ui.data(|d| d.get_temp(memo));
        if last == Some(self.st.scope) {
            return;
        }
        ui.data_mut(|d| d.insert_temp(memo, self.st.scope));
        if let Scope::Project(id) = self.st.scope {
            let node = ui.make_persistent_id(("sb-project", id));
            let mut state = CollapsingState::load_with_default_open(ui.ctx(), node, false);
            state.set_open(true);
            state.store(ui.ctx());
        }
    }

    /// Un proyecto: cabecera (clic = elegirlo; clic derecho = sus acciones)
    /// y, desplegado, sus tareas, ciclos, épicas, hitos y vistas.
    fn project_node(
        &self,
        ui: &mut egui::Ui,
        p: &Project,
        in_tasks: bool,
        pick: &mut Option<Pick>,
        out: &mut Vec<Intent>,
    ) {
        let lang = self.st.lang;
        let here = self.st.scope == Scope::Project(p.id);
        let id = ui.make_persistent_id(("sb-project", p.id));
        let state = CollapsingState::load_with_default_open(ui.ctx(), id, false);
        let collapsed = !state.is_open();
        let count = self
            .snap
            .open_counts
            .get(&p.id)
            .map_or_else(String::new, ToString::to_string);
        let lead = if p.locked {
            Lead::Icon(Icon::Lock, theme::GRAY)
        } else {
            Lead::None
        };
        let mut clicked = false;
        state
            .show_header(ui, |ui| {
                let name = format!("{} · {}", p.key, p.name);
                let r = row(ui, lead, &name, &count, here && in_tasks && collapsed);
                clicked = r.clicked();
                r.context_menu(|ui| self.project_menu(ui, p, pick));
            })
            .body(|ui| {
                let plain = here
                    && in_tasks
                    && self.st.filter.cycle.is_none()
                    && self.st.filter.epic.is_none()
                    && self.st.active_view.is_none();
                if row(
                    ui,
                    Lead::Icon(view_icon(self.st.view), theme::GRAY),
                    &t(lang, "ui-sidebar-tasks"),
                    &count,
                    plain,
                )
                .clicked()
                {
                    out.push(Intent::GoTo(GoTo::project(Scope::Project(p.id))));
                }
                self.cycle_rows(ui, p, in_tasks, pick, out);
                for kind in EpicKind::ALL {
                    self.epic_rows(ui, p, *kind, in_tasks, out);
                }
                let diagrams = self.snap.diagrams_of(p.id).count();
                if diagrams > 0
                    && row(
                        ui,
                        Lead::Icon(Icon::Diagram, theme::GRAY),
                        &t(lang, "ui-view-diagrams"),
                        &diagrams.to_string(),
                        here && self.st.view == ViewKind::Diagrams,
                    )
                    .clicked()
                {
                    out.push(Intent::Diagrams(p.id));
                }
                let views: Vec<_> = self
                    .snap
                    .views
                    .iter()
                    .filter(|v| v.project_id == Some(p.id))
                    .collect();
                if !views.is_empty() {
                    let head = t(lang, "ui-sidebar-views");
                    section(
                        ui,
                        ("sb-views", p.id),
                        false,
                        (
                            Lead::Icon(Icon::Bookmark, theme::GRAY),
                            &head,
                            &views.len().to_string(),
                        ),
                        None,
                        |ui| self.view_rows(ui, &views, pick),
                    );
                }
            });
        if clicked {
            let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, false);
            state.set_open(true);
            state.store(ui.ctx());
            out.push(Intent::GoTo(GoTo::project(Scope::Project(p.id))));
        }
    }

    fn project_menu(&self, ui: &mut egui::Ui, p: &Project, pick: &mut Option<Pick>) {
        let lang = self.st.lang;
        let mut chosen = None;
        if ui.button(t(lang, "ui-edit-project")).clicked() {
            chosen = Some(Pick::Dialog(Dialog::Project(ProjectForm::edit(p))));
        }
        if ui.button(t(lang, "ui-workflow-fields")).clicked() {
            chosen = Some(Pick::Dialog(Dialog::Workflow(
                crate::state::WorkflowForm::new(p.id),
            )));
        }
        if ui.button(t(lang, "ui-cycle-new")).clicked() {
            let form = CycleForm::after_last(&self.snap, p.id, today());
            chosen = Some(Pick::Dialog(Dialog::Cycle(form)));
        }
        if ui.button(t(lang, "ui-epic-new")).clicked() {
            // La vista de épicas tiene el alta; con el proyecto ya elegido.
            chosen = Some(Pick::View(ViewKind::Epics));
        }
        if chosen.is_some() {
            *pick = chosen;
            ui.close();
        }
    }

    /// Ciclos abiertos del proyecto, con su progreso. Siempre visible, para
    /// poder crear el primero desde aquí.
    fn cycle_rows(
        &self,
        ui: &mut egui::Ui,
        p: &Project,
        in_tasks: bool,
        pick: &mut Option<Pick>,
        out: &mut Vec<Intent>,
    ) {
        let lang = self.st.lang;
        let today = today();
        let cycles: Vec<_> = self.snap.open_cycles(p.id).collect();
        let head = t(lang, "ui-view-cycles");
        let added = section(
            ui,
            ("sb-cycles", p.id),
            true,
            (Lead::Icon(Icon::Cycle, theme::GRAY), &head, ""),
            Some(&t(lang, "ui-cycle-new")),
            |ui| {
                if cycles.is_empty() {
                    ui.weak(t(lang, "ui-sidebar-no-cycles"));
                }
                for c in &cycles {
                    let state = c.state(today);
                    let selected = in_tasks && self.st.filter.cycle == Some(c.id);
                    let r = row(
                        ui,
                        Lead::Dot(state_color(state)),
                        &c.name,
                        &progress(self.snap.cycle_progress(c.id)),
                        selected,
                    )
                    .on_hover_text(format!("{} – {}", c.start, c.end));
                    if r.clicked() {
                        out.push(Intent::GoTo(GoTo::cycle(c)));
                    }
                    r.context_menu(|ui| {
                        if ui.button(t(lang, "ui-cycle-edit")).clicked() {
                            *pick = Some(Pick::Dialog(Dialog::Cycle(CycleForm::edit(c))));
                            ui.close();
                        }
                        if ui
                            .button(t(lang, "ui-cycle-close"))
                            .on_hover_text(t(lang, "ui-cycle-close-hint"))
                            .clicked()
                        {
                            out.push(Intent::run("cycle.close", &[("cycle", &c.id.to_string())]));
                            ui.close();
                        }
                        if ui.button(t(lang, "ui-cycle-delete")).clicked() {
                            *pick = Some(Pick::Dialog(Dialog::ConfirmCycleDelete(
                                c.id,
                                c.name.clone(),
                            )));
                            ui.close();
                        }
                    });
                }
            },
        );
        if added {
            let form = CycleForm::after_last(&self.snap, p.id, today);
            *pick = Some(Pick::Dialog(Dialog::Cycle(form)));
        }
    }

    /// Épicas o hitos abiertos del proyecto; sin ninguno, no se muestra.
    fn epic_rows(
        &self,
        ui: &mut egui::Ui,
        p: &Project,
        kind: EpicKind,
        in_tasks: bool,
        out: &mut Vec<Intent>,
    ) {
        let lang = self.st.lang;
        let lines: Vec<_> = self
            .snap
            .groups_of(p.id, kind)
            .into_iter()
            .filter(|e| e.epic.status == EpicStatus::Open)
            .collect();
        if lines.is_empty() {
            return;
        }
        let (icon, head) = match kind {
            EpicKind::Epic => (Icon::Epic, "ui-sidebar-epics"),
            EpicKind::Milestone => (Icon::Milestone, "ui-sidebar-milestones"),
        };
        let head = t(lang, head);
        section(
            ui,
            ("sb-epics", p.id, kind.as_str()),
            false,
            (
                Lead::Icon(icon, theme::VIOLET),
                &head,
                &lines.len().to_string(),
            ),
            None,
            |ui| {
                for line in lines {
                    let e = &line.epic;
                    let selected = in_tasks && self.st.filter.epic == Some(e.id);
                    let mut r = row(
                        ui,
                        Lead::None,
                        &e.name,
                        &progress((line.done, line.total)),
                        selected,
                    );
                    if let Some(target) = &e.target {
                        r = r.on_hover_text(target.date().to_string());
                    }
                    if r.clicked() {
                        out.push(Intent::GoTo(GoTo::epic(e)));
                    }
                }
            },
        );
    }

    fn view_rows(&self, ui: &mut egui::Ui, views: &[&model::SavedView], pick: &mut Option<Pick>) {
        for v in views {
            let on = self.st.active_view.as_deref() == Some(v.name.as_str());
            if row(ui, Lead::None, &v.name, "", on)
                .on_hover_text(&v.filter)
                .clicked()
            {
                *pick = Some(Pick::Saved((*v).clone()));
            }
        }
    }

    /// Lleva a las tareas de `to`. Fuera de las vistas de tareas abre el
    /// tablero si es un ciclo y la lista si no; dentro, se queda en la vista.
    pub(super) fn go_to(&mut self, to: GoTo) {
        self.st.panels.sidebar_peek = false;
        self.st.filter.cycle = to.cycle;
        self.st.filter.epic = to.epic;
        self.st.active_view = None;
        if !ViewKind::TASK_VIEWS.contains(&self.st.view) {
            self.st.view = if to.cycle.is_some() {
                ViewKind::Board
            } else {
                ViewKind::List
            };
        }
        if self.st.scope != to.scope {
            self.st.scope = to.scope;
            self.st.selected = None;
            self.reload();
        }
    }

    /// Secciones fuera de las vistas de tareas, con su contador.
    pub(super) fn nav_items(&self) -> [(ViewKind, Icon, String); 8] {
        let open_cycles = self
            .snap
            .cycles
            .iter()
            .filter(|c| c.closed_at.is_none())
            .count();
        [
            (
                ViewKind::Review,
                Icon::Review,
                self.snap.review.len().to_string(),
            ),
            (
                ViewKind::Inbox,
                Icon::Inbox,
                self.snap.open_inbox().to_string(),
            ),
            (
                ViewKind::Agents,
                Icon::Agents,
                format!(
                    "{}/{}",
                    self.snap.agents_connected(),
                    self.snap.agents.len()
                ),
            ),
            (ViewKind::Cycles, Icon::Cycle, open_cycles.to_string()),
            (
                ViewKind::Epics,
                Icon::Epic,
                self.snap
                    .epics
                    .iter()
                    .filter(|e| e.epic.status == EpicStatus::Open)
                    .count()
                    .to_string(),
            ),
            (
                ViewKind::Diagrams,
                Icon::Diagram,
                self.snap.diagrams.len().to_string(),
            ),
            (ViewKind::Metrics, Icon::Chart, String::new()),
            (
                ViewKind::Trash,
                Icon::Trash,
                self.snap.trash.len().to_string(),
            ),
        ]
    }

    /// Barra lateral plegada: solo iconos, con el nombre en el tooltip y un
    /// contador en lo que pide atención (revisión y bandeja).
    pub(super) fn rail(&mut self, ui: &mut egui::Ui, can_dock: bool) {
        let lang = self.st.lang;
        ui.vertical_centered(|ui| {
            let tip = t(lang, "ui-sidebar-expand");
            if icons::tab(ui, Icon::Menu, None, self.st.panels.sidebar_peek, &tip).clicked() {
                if can_dock {
                    self.st.panels.sidebar_collapsed = false;
                } else {
                    self.st.panels.sidebar_peek = !self.st.panels.sidebar_peek;
                }
            }
            ui.separator();
            let in_tasks = ViewKind::TASK_VIEWS.contains(&self.st.view);
            let icon = if in_tasks {
                view_icon(self.st.view)
            } else {
                Icon::List
            };
            if icons::tab(ui, icon, None, in_tasks, &t(lang, "ui-tasks")).clicked() && !in_tasks {
                self.st.view = ViewKind::List;
            }
            for (view, icon, count) in self.nav_items() {
                let label = t(lang, view.label_key());
                let name = if count.is_empty() {
                    label
                } else {
                    format!("{label} ({count})")
                };
                let r = icons::tab(ui, icon, None, self.st.view == view, &name);
                if r.clicked() {
                    self.st.view = view;
                }
                let color = match view {
                    ViewKind::Review => theme::VIOLET,
                    ViewKind::Inbox => theme::BLUE,
                    _ => continue,
                };
                if count != "0" {
                    let at = r.rect.right_top() + egui::vec2(-3.0, 3.0);
                    ui.painter().circle_filled(at, 7.0, color);
                    ui.painter().text(
                        at,
                        egui::Align2::CENTER_CENTER,
                        &count,
                        egui::FontId::proportional(10.0),
                        egui::Color32::WHITE,
                    );
                }
            }
        });
    }

    /// La barra lateral completa encima de la vista, cuando no cabe al lado.
    pub(super) fn sidebar_overlay(
        &mut self,
        ctx: &egui::Context,
        area: egui::Rect,
        was_open: bool,
        out: &mut Vec<Intent>,
    ) {
        if !self.st.panels.sidebar_peek {
            return;
        }
        let scale = self.st.text_size.scale();
        let shown = egui::Area::new(egui::Id::unique("taskcraft-sidebar-peek"))
            .order(egui::Order::Foreground)
            .fixed_pos(area.left_top())
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(layout::SIDEBAR * scale);
                    ui.set_height((area.height() - 16.0).max(120.0));
                    self.sidebar(ui, false, out);
                });
            });
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape || (was_open && shown.response.clicked_elsewhere()) {
            self.st.panels.sidebar_peek = false;
        }
    }
}
