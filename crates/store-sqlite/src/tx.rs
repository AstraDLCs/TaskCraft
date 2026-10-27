use std::collections::{HashMap, HashSet};

use model::{
    ActivityEntry, Agent, AgentId, AgentSession, CheckItem, Comment, Dependency, InboxId,
    InboxItem, InboxStatus, Label, LabelId, Project, ProjectId, SessionId, Status, Task, TaskId,
    WorkspaceInfo,
};
use rusqlite::{OptionalExtension, Params, Row, params, params_from_iter};
use store::{ActivityFilter, Reader, StoreError, StoreResult, TaskFilter, Transaction, Writer};

use crate::db;
use crate::rows::{
    ActivityRow, AgentRow, AttachmentRow, CommentRow, CostRow, CycleRow, DiagramRow, EpicRow,
    GitRow, InboxRow, InterfaceRow, ProjectRow, SessionRow, TaskParts, TaskRow, TemplateRow,
    TimeRow, WorkspaceRow, json, parse, text, to_json,
};

const CHECKLIST: &str = "checklist";
const CRITERION: &str = "criterion";

/// Transacción SQLite. De lectura (diferida) o de escritura (inmediata);
/// si se descarta sin `commit`, SQLite hace rollback.
pub(crate) struct SqliteTx<'a> {
    tx: rusqlite::Transaction<'a>,
    commits: Option<&'a mut u64>,
}

impl<'a> SqliteTx<'a> {
    pub fn new(tx: rusqlite::Transaction<'a>, commits: Option<&'a mut u64>) -> Self {
        Self { tx, commits }
    }

    fn rows<T, P: Params>(
        &self,
        sql: &str,
        params: P,
        f: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
    ) -> StoreResult<Vec<T>> {
        let mut stmt = self.tx.prepare_cached(sql).map_err(db)?;
        let rows = stmt.query_map(params, f).map_err(db)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db)
    }

    fn ids<T: std::str::FromStr, P: Params>(&self, sql: &str, params: P) -> StoreResult<Vec<T>>
    where
        T::Err: std::fmt::Display,
    {
        self.rows(sql, params, |r| r.get::<_, String>(0))?
            .iter()
            .map(|id| parse("id", id))
            .collect()
    }

    /// Filas de tareas con su checklist, criterios y etiquetas. Con varias
    /// filas, las partes se leen por lotes y no tarea a tarea (RNF-06).
    fn load_tasks(&self, sql: &str, params: impl Params) -> StoreResult<Vec<Task>> {
        let rows = self.rows(sql, params, TaskRow::read)?;
        if rows.len() <= 1 {
            return rows
                .into_iter()
                .map(|row| {
                    let parts = self.task_parts(&row.id)?;
                    row.into_model(parts)
                })
                .collect();
        }
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        let mut parts = self.task_parts_batch(&ids)?;
        rows.into_iter()
            .map(|row| {
                let p = parts.remove(&row.id).unwrap_or_default();
                row.into_model(p)
            })
            .collect()
    }

    /// Partes de muchas tareas, en bloques de `PARTS_CHUNK` IDs por consulta.
    fn task_parts_batch(&self, ids: &[&str]) -> StoreResult<HashMap<String, TaskParts>> {
        let mut out: HashMap<String, TaskParts> = HashMap::with_capacity(ids.len());
        for chunk in ids.chunks(PARTS_CHUNK) {
            let slots = vec!["?"; chunk.len()].join(", ");
            let items = self.rows(
                &format!(
                    "SELECT task_id, id, kind, text, done FROM task_item
                     WHERE task_id IN ({slots}) ORDER BY task_id, kind, position"
                ),
                params_from_iter(chunk.iter()),
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, bool>(4)?,
                    ))
                },
            )?;
            for (task, id, kind, text, done) in items {
                let item = CheckItem {
                    id: parse("task_item.id", &id)?,
                    text,
                    done,
                };
                let entry = out.entry(task).or_default();
                match kind.as_str() {
                    CHECKLIST => entry.checklist.push(item),
                    CRITERION => entry.criteria.push(item),
                    other => {
                        return Err(StoreError::Corrupt(format!("task_item.kind = `{other}`")));
                    }
                }
            }
            let labels = self.rows(
                &format!(
                    "SELECT tl.task_id, tl.label_id FROM task_label tl JOIN label l ON l.id = tl.label_id
                     WHERE tl.task_id IN ({slots}) ORDER BY tl.task_id, l.name"
                ),
                params_from_iter(chunk.iter()),
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )?;
            for (task, label) in labels {
                out.entry(task)
                    .or_default()
                    .labels
                    .push(parse("task_label.label_id", &label)?);
            }
        }
        Ok(out)
    }

    fn task_parts(&self, task_id: &str) -> StoreResult<TaskParts> {
        let mut checklist = Vec::new();
        let mut criteria = Vec::new();
        let items = self.rows(
            "SELECT id, kind, text, done FROM task_item WHERE task_id = ?1 ORDER BY kind, position",
            [task_id],
            |r| {
                Ok((
                    r.get::<_, String>("id")?,
                    r.get::<_, String>("kind")?,
                    r.get::<_, String>("text")?,
                    r.get::<_, bool>("done")?,
                ))
            },
        )?;
        for (id, kind, text, done) in items {
            let item = CheckItem {
                id: parse("task_item.id", &id)?,
                text,
                done,
            };
            match kind.as_str() {
                CHECKLIST => checklist.push(item),
                CRITERION => criteria.push(item),
                other => return Err(StoreError::Corrupt(format!("task_item.kind = `{other}`"))),
            }
        }
        let labels = self.ids(
            "SELECT tl.label_id FROM task_label tl JOIN label l ON l.id = tl.label_id
             WHERE tl.task_id = ?1 ORDER BY l.name",
            [task_id],
        )?;
        Ok(TaskParts {
            checklist,
            criteria,
            labels,
        })
    }

    /// Columnas añadidas en v5 (D10, RF-MOD-13/14/15).
    fn write_task_extra(&self, t: &Task) -> StoreResult<()> {
        self.tx
            .prepare_cached(
                "UPDATE task SET stage = ?2, estimate = ?3, epic_id = ?4, milestone_id = ?5,
                        fields = ?6, assignees = ?7, recurrence = ?8, cycles = ?9
                 WHERE id = ?1",
            )
            .and_then(|mut st| {
                st.execute(params![
                    t.id.to_string(),
                    t.stage,
                    t.estimate.map(|e| e.0),
                    text(&t.epic_id),
                    text(&t.milestone_id),
                    to_json(&t.fields),
                    to_json(&t.assignees),
                    t.recurrence.as_ref().map(to_json),
                    to_json(&t.cycles),
                ])
            })
            .map_err(db)?;
        Ok(())
    }

    /// Citas de diagramas con `column` (`source_id` o `target_id`) = `id`.
    fn diagram_mentions_where(
        &self,
        column: &str,
        id: String,
    ) -> StoreResult<Vec<model::DiagramMention>> {
        let sql = format!(
            "SELECT source_id, comment_id, target_id FROM diagram_mention
             WHERE {column} = ?1 ORDER BY source_id, target_id"
        );
        let rows = self.rows(&sql, [id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        rows.into_iter()
            .map(|(source, comment, target)| {
                Ok(model::DiagramMention {
                    source: parse("diagram_mention.source_id", &source)?,
                    comment: comment
                        .map(|c| parse("diagram_mention.comment_id", &c))
                        .transpose()?,
                    target: parse("diagram_mention.target_id", &target)?,
                })
            })
            .collect()
    }

    fn write_project_extra(&self, p: &Project) -> StoreResult<()> {
        self.tx
            .execute(
                "UPDATE project SET workflow = ?2, estimate_unit = ?3, fields = ?4,
                        next_diagram = ?5
                 WHERE id = ?1",
                params![
                    p.id.to_string(),
                    to_json(&p.workflow),
                    p.estimate_unit.as_str(),
                    to_json(&p.fields),
                    p.next_diagram,
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn write_task_parts(&self, task: &Task) -> StoreResult<()> {
        let id = task.id.to_string();
        self.tx
            .execute("DELETE FROM task_item WHERE task_id = ?1", [&id])
            .map_err(db)?;
        self.tx
            .execute("DELETE FROM task_label WHERE task_id = ?1", [&id])
            .map_err(db)?;

        let mut insert_item = self
            .tx
            .prepare_cached(
                "INSERT INTO task_item (id, task_id, kind, position, text, done)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(db)?;
        for (kind, items) in [(CHECKLIST, &task.checklist), (CRITERION, &task.criteria)] {
            for (position, item) in (0u32..).zip(items) {
                insert_item
                    .execute(params![
                        item.id.to_string(),
                        id,
                        kind,
                        position,
                        item.text,
                        item.done
                    ])
                    .map_err(db)?;
            }
        }

        let mut insert_label = self
            .tx
            .prepare_cached("INSERT INTO task_label (task_id, label_id) VALUES (?1, ?2)")
            .map_err(db)?;
        for label in &task.labels {
            insert_label
                .execute(params![id, label.to_string()])
                .map_err(db)?;
        }
        Ok(())
    }

    /// Las 13 columnas de una interfaz, en orden.
    /// Abre la puerta del historial (D23) solo mientras dura `f`, dentro de
    /// esta transacción: fuera de ella, borrar historial sigue prohibido.
    fn with_activity_gate<T>(
        &self,
        f: impl FnOnce(&rusqlite::Transaction<'_>) -> rusqlite::Result<T>,
    ) -> StoreResult<T> {
        self.tx
            .execute("INSERT INTO activity_gate (open) VALUES (1)", [])
            .map_err(db)?;
        let out = f(&self.tx).map_err(db);
        self.tx
            .execute("DELETE FROM activity_gate", [])
            .map_err(db)?;
        out
    }

    fn exec_interface(&self, sql: &str, i: &model::Interface) -> StoreResult<usize> {
        self.tx
            .execute(
                sql,
                params![
                    i.id.to_string(),
                    i.name,
                    i.kind,
                    i.description,
                    i.color,
                    i.status.as_str(),
                    i.level.as_str(),
                    to_json(&i.scope),
                    i.max_agents,
                    i.paused,
                    i.token_hash,
                    i.created_at.to_string(),
                    i.updated_at.to_string(),
                    i.token,
                ],
            )
            .map_err(db)
    }

    fn one_project(&self, sql: &str, param: &str) -> StoreResult<Option<Project>> {
        self.tx
            .query_row(sql, [param], ProjectRow::read)
            .optional()
            .map_err(db)?
            .map(ProjectRow::into_model)
            .transpose()
    }
}

impl Reader for SqliteTx<'_> {
    fn workspace(&self) -> StoreResult<WorkspaceInfo> {
        self.tx
            .query_row(WorkspaceRow::SELECT, [], WorkspaceRow::read)
            .map_err(db)?
            .into_model()
    }

    fn project(&self, id: ProjectId) -> StoreResult<Option<Project>> {
        let sql = format!("{} WHERE id = ?1", ProjectRow::SELECT);
        self.one_project(&sql, &id.to_string())
    }

    fn project_by_key(&self, key: &str) -> StoreResult<Option<Project>> {
        let sql = format!("{} WHERE key = ?1", ProjectRow::SELECT);
        self.one_project(&sql, &key.to_uppercase())
    }

    fn projects(&self, include_archived: bool) -> StoreResult<Vec<Project>> {
        let sql = format!(
            "{} WHERE ?1 OR status = 'active' ORDER BY key",
            ProjectRow::SELECT
        );
        self.rows(&sql, [include_archived], ProjectRow::read)?
            .into_iter()
            .map(ProjectRow::into_model)
            .collect()
    }

    fn task(&self, id: TaskId) -> StoreResult<Option<Task>> {
        let sql = format!("{} WHERE id = ?1", TaskRow::SELECT);
        Ok(self.load_tasks(&sql, [id.to_string()])?.into_iter().next())
    }

    fn task_by_number(&self, project: ProjectId, number: u64) -> StoreResult<Option<Task>> {
        let sql = format!("{} WHERE project_id = ?1 AND number = ?2", TaskRow::SELECT);
        Ok(self
            .load_tasks(&sql, params![project.to_string(), number])?
            .into_iter()
            .next())
    }

    fn tasks(&self, filter: &TaskFilter) -> StoreResult<Vec<Task>> {
        let mut conditions = Vec::new();
        let mut values: Vec<String> = Vec::new();
        if let Some(project) = filter.project {
            values.push(project.to_string());
            conditions.push(format!("project_id = ?{}", values.len()));
        }
        match filter.parent {
            Some(None) => conditions.push("parent_id IS NULL".into()),
            Some(Some(parent)) => {
                values.push(parent.to_string());
                conditions.push(format!("parent_id = ?{}", values.len()));
            }
            None => {}
        }
        if !filter.statuses.is_empty() {
            let mut slots = Vec::new();
            for status in &filter.statuses {
                values.push(status.to_string());
                slots.push(format!("?{}", values.len()));
            }
            conditions.push(format!("status IN ({})", slots.join(", ")));
        }
        if !filter.include_archived {
            conditions.push("archived_at IS NULL".into());
        }
        conditions.push(if filter.deleted {
            "deleted_at IS NOT NULL".into()
        } else {
            "deleted_at IS NULL".into()
        });
        if filter.needs_human {
            conditions.push("needs_human IS NOT NULL".into());
        }
        if let Some(group) = filter.group {
            values.push(group.to_string());
            let n = values.len();
            conditions.push(format!("(epic_id = ?{n} OR milestone_id = ?{n})"));
        }
        if let Some(cycle) = filter.cycle {
            values.push(cycle.to_string());
            conditions.push(format!(
                "EXISTS (SELECT 1 FROM json_each(task.cycles) WHERE value = ?{})",
                values.len()
            ));
        }

        let sql = format!(
            "{} WHERE {} ORDER BY project_id, number",
            TaskRow::SELECT,
            conditions.join(" AND ")
        );
        self.load_tasks(&sql, params_from_iter(values))
    }

    fn children_summary(&self, closed: &[Status]) -> StoreResult<HashMap<TaskId, (usize, usize)>> {
        let sql = format!(
            "SELECT parent_id, COUNT(*), SUM(status IN ({})) FROM task
             WHERE parent_id IS NOT NULL AND deleted_at IS NULL GROUP BY parent_id",
            status_list(closed)
        );
        let rows = self.rows(&sql, [], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
            ))
        })?;
        rows.into_iter()
            .map(|(id, total, done)| {
                Ok((parse("task.parent_id", &id)?, (count(total), count(done))))
            })
            .collect()
    }

    fn open_counts(&self, closed: &[Status]) -> StoreResult<HashMap<ProjectId, usize>> {
        let sql = format!(
            "SELECT project_id, COUNT(*) FROM task
             WHERE deleted_at IS NULL AND archived_at IS NULL AND status NOT IN ({})
             GROUP BY project_id",
            status_list(closed)
        );
        let rows = self.rows(&sql, [], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        rows.into_iter()
            .map(|(id, n)| Ok((parse("task.project_id", &id)?, count(n))))
            .collect()
    }

    fn blocked_tasks(&self, closed: &[Status]) -> StoreResult<HashSet<TaskId>> {
        let sql = format!(
            "SELECT DISTINCT d.blocked_id FROM task_dependency d
             JOIN task b ON b.id = d.blocker_id
             WHERE b.deleted_at IS NULL AND b.status NOT IN ({})",
            status_list(closed)
        );
        Ok(self.ids(&sql, [])?.into_iter().collect())
    }

    fn locked_subtrees(&self) -> StoreResult<HashSet<TaskId>> {
        let sql = "WITH RECURSIVE locked(id) AS (
                       SELECT id FROM task WHERE locked = 1
                       UNION SELECT t.id FROM task t JOIN locked l ON t.parent_id = l.id
                   ) SELECT id FROM locked";
        Ok(self.ids(sql, [])?.into_iter().collect())
    }

    fn next_candidates(
        &self,
        filter: &store::NextFilter,
    ) -> StoreResult<Vec<store::NextCandidate>> {
        // Listas de IDs (alcance y capacidades) como texto fijo: son ULID ya
        // validados por el modelo, no texto del usuario.
        let ids = |list: &[String]| {
            list.iter()
                .map(|id| format!("'{id}'"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let projects: Vec<String> = filter.projects.iter().map(ToString::to_string).collect();
        let labels: Vec<String> = filter.labels.iter().map(ToString::to_string).collect();
        let scope = if projects.is_empty() {
            String::new()
        } else {
            format!("AND t.project_id IN ({})", ids(&projects))
        };
        let skills = if labels.is_empty() {
            String::new()
        } else {
            format!(
                "AND (NOT EXISTS (SELECT 1 FROM task_label tl WHERE tl.task_id = t.id)
                      OR EXISTS (SELECT 1 FROM task_label tl WHERE tl.task_id = t.id
                                 AND tl.label_id IN ({})))",
                ids(&labels)
            )
        };
        let sql = format!(
            "WITH RECURSIVE locked(id) AS (
                 SELECT id FROM task WHERE locked = 1
                 UNION SELECT t.id FROM task t JOIN locked l ON t.parent_id = l.id
             )
             SELECT t.id, t.priority, t.due_at, t.number, t.lease_until, t.assignees,
                    t.claimed_by, t.project_id, t.stage
             FROM task t JOIN project p ON p.id = t.project_id
             WHERE t.status = ?1 AND t.deleted_at IS NULL AND t.archived_at IS NULL
               AND t.needs_human IS NULL AND p.locked = 0
               {scope} {skills}
               AND t.id NOT IN (SELECT id FROM locked)
               AND NOT EXISTS (
                   SELECT 1 FROM epic e WHERE e.locked = 1
                     AND (e.id = t.epic_id OR e.id = t.milestone_id))
               AND NOT EXISTS (
                   SELECT 1 FROM task_dependency d JOIN task b ON b.id = d.blocker_id
                   WHERE d.blocked_id = t.id AND b.deleted_at IS NULL
                     AND b.status NOT IN ({}))",
            status_list(&filter.closed)
        );
        let rows = self.rows(&sql, params![filter.status.as_str()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, u64>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, Option<String>>(8)?,
            ))
        })?;
        rows.into_iter()
            .map(
                |(id, priority, due, number, lease, assignees, claimed, project, stage)| {
                    Ok(store::NextCandidate {
                        id: parse("task.id", &id)?,
                        priority: parse("task.priority", &priority)?,
                        due: due.map(|d| parse("task.due_at", &d)).transpose()?,
                        number,
                        lease_until: lease.map(|l| parse("task.lease_until", &l)).transpose()?,
                        assignees: json("task.assignees", &assignees)?,
                        claimed_by: claimed.map(|c| parse("task.claimed_by", &c)).transpose()?,
                        project_id: parse("task.project_id", &project)?,
                        stage,
                    })
                },
            )
            .collect()
    }

    fn idempotent(&self, actor: &model::Actor, key: &str) -> StoreResult<Option<(String, String)>> {
        self.tx
            .query_row(
                "SELECT command, response FROM idempotency WHERE actor = ?1 AND key = ?2",
                params![actor.to_string(), key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(db)
    }

    fn comment_counts(&self) -> StoreResult<HashMap<TaskId, usize>> {
        let rows = self.rows(
            "SELECT task_id, COUNT(*) FROM comment GROUP BY task_id",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )?;
        rows.into_iter()
            .map(|(id, n)| Ok((parse("comment.task_id", &id)?, count(n))))
            .collect()
    }

    fn children(&self, parent: TaskId) -> StoreResult<Vec<Task>> {
        let sql = format!("{} WHERE parent_id = ?1 ORDER BY number", TaskRow::SELECT);
        self.load_tasks(&sql, [parent.to_string()])
    }

    fn label(&self, id: LabelId) -> StoreResult<Option<Label>> {
        self.rows(
            "SELECT id, name, color FROM label WHERE id = ?1",
            [id.to_string()],
            read_label,
        )?
        .into_iter()
        .next()
        .transpose()
    }

    fn label_by_name(&self, name: &str) -> StoreResult<Option<Label>> {
        self.rows(
            "SELECT id, name, color FROM label WHERE name = ?1",
            [name],
            read_label,
        )?
        .into_iter()
        .next()
        .transpose()
    }

    fn labels(&self) -> StoreResult<Vec<Label>> {
        self.rows(
            "SELECT id, name, color FROM label ORDER BY name",
            [],
            read_label,
        )?
        .into_iter()
        .collect()
    }

    fn dependencies(&self) -> StoreResult<Vec<Dependency>> {
        self.rows(
            "SELECT blocker_id, blocked_id FROM task_dependency",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?
        .into_iter()
        .map(|(blocker, blocked)| {
            Ok(Dependency {
                blocker: parse("task_dependency.blocker_id", &blocker)?,
                blocked: parse("task_dependency.blocked_id", &blocked)?,
            })
        })
        .collect()
    }

    fn blockers_of(&self, task: TaskId) -> StoreResult<Vec<TaskId>> {
        self.ids(
            "SELECT blocker_id FROM task_dependency WHERE blocked_id = ?1",
            [task.to_string()],
        )
    }

    fn blocks(&self, task: TaskId) -> StoreResult<Vec<TaskId>> {
        self.ids(
            "SELECT blocked_id FROM task_dependency WHERE blocker_id = ?1",
            [task.to_string()],
        )
    }

    fn agent(&self, id: AgentId) -> StoreResult<Option<Agent>> {
        let sql = format!("{} WHERE id = ?1", AgentRow::SELECT);
        first(
            self.rows(&sql, [id.to_string()], AgentRow::read)?,
            AgentRow::into_model,
        )
    }

    fn agent_by_name(&self, name: &str) -> StoreResult<Option<Agent>> {
        let sql = format!("{} WHERE name = ?1", AgentRow::SELECT);
        first(
            self.rows(&sql, [name.to_lowercase()], AgentRow::read)?,
            AgentRow::into_model,
        )
    }

    fn interface(&self, id: model::InterfaceId) -> StoreResult<Option<model::Interface>> {
        let sql = format!("{} WHERE id = ?1", InterfaceRow::SELECT);
        first(
            self.rows(&sql, [id.to_string()], InterfaceRow::read)?,
            InterfaceRow::into_model,
        )
    }

    fn interface_by_name(&self, name: &str) -> StoreResult<Option<model::Interface>> {
        // Se guarda tal como se escribió (D22) y se busca sin mayúsculas.
        let sql = format!("{} WHERE lower(name) = ?1", InterfaceRow::SELECT);
        first(
            self.rows(&sql, [name.trim().to_lowercase()], InterfaceRow::read)?,
            InterfaceRow::into_model,
        )
    }

    fn interface_by_token(&self, token_hash: &str) -> StoreResult<Option<model::Interface>> {
        let sql = format!("{} WHERE token_hash = ?1", InterfaceRow::SELECT);
        first(
            self.rows(&sql, [token_hash], InterfaceRow::read)?,
            InterfaceRow::into_model,
        )
    }

    fn interfaces(&self) -> StoreResult<Vec<model::Interface>> {
        let sql = format!("{} ORDER BY name", InterfaceRow::SELECT);
        self.rows(&sql, [], InterfaceRow::read)?
            .into_iter()
            .map(InterfaceRow::into_model)
            .collect()
    }

    fn search_tasks(
        &self,
        query: &str,
        project: Option<ProjectId>,
        limit: usize,
    ) -> StoreResult<Vec<TaskId>> {
        let Some(fts) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let sql = format!(
            "SELECT t.id FROM task_fts f JOIN task t ON t.rowid = f.rowid
             WHERE task_fts MATCH ?1 AND (?2 IS NULL OR t.project_id = ?2)
               AND t.deleted_at IS NULL
             ORDER BY rank LIMIT {limit}"
        );
        self.ids(&sql, params![fts, project.map(|p| p.to_string())])
    }

    fn agents(&self) -> StoreResult<Vec<Agent>> {
        let sql = format!("{} ORDER BY name", AgentRow::SELECT);
        self.rows(&sql, [], AgentRow::read)?
            .into_iter()
            .map(AgentRow::into_model)
            .collect()
    }

    fn session(&self, id: SessionId) -> StoreResult<Option<AgentSession>> {
        let sql = format!("{} WHERE id = ?1", SessionRow::SELECT);
        first(
            self.rows(&sql, [id.to_string()], SessionRow::read)?,
            SessionRow::into_model,
        )
    }

    fn open_sessions(&self, agent: Option<AgentId>) -> StoreResult<Vec<AgentSession>> {
        let sql = format!(
            "{} WHERE ended_at IS NULL AND (?1 IS NULL OR agent_id = ?1) ORDER BY started_at",
            SessionRow::SELECT
        );
        self.rows(&sql, [agent.map(|a| a.to_string())], SessionRow::read)?
            .into_iter()
            .map(SessionRow::into_model)
            .collect()
    }

    fn comments(&self, task: TaskId) -> StoreResult<Vec<Comment>> {
        let sql = format!("{} WHERE task_id = ?1 ORDER BY id", CommentRow::SELECT);
        self.rows(&sql, [task.to_string()], CommentRow::read)?
            .into_iter()
            .map(CommentRow::into_model)
            .collect()
    }

    fn comment_count(&self, task: TaskId) -> StoreResult<usize> {
        self.tx
            .query_row(
                "SELECT COUNT(*) FROM comment WHERE task_id = ?1",
                [task.to_string()],
                |r| r.get(0),
            )
            .map_err(db)
    }

    fn activity(&self, filter: &ActivityFilter) -> StoreResult<Vec<ActivityEntry>> {
        let mut conditions = vec!["1 = 1".to_owned()];
        let mut values: Vec<String> = Vec::new();
        let mut push = |column: &str, op: &str, value: String| {
            values.push(value);
            conditions.push(format!("{column} {op} ?{}", values.len()));
        };
        if let Some(task) = filter.task {
            push("task_id", "=", task.to_string());
        }
        if let Some(project) = filter.project {
            push("project_id", "=", project.to_string());
        }
        if let Some(actor) = &filter.actor {
            push("actor", "=", actor.to_string());
        }
        if let Some(entity) = filter.entity {
            push("entity_kind", "=", entity.as_str().to_owned());
        }
        if let Some(correlation) = filter.correlation {
            push("correlation", "=", correlation.to_string());
        }
        if let Some(before) = filter.before {
            push("id", "<", before.to_string());
        }
        if let Some(since) = filter.since {
            // Los IDs son ULID: el instante va en sus primeros caracteres.
            let ms = u64::try_from(since.as_millisecond()).unwrap_or(0);
            let floor = foundation::Ulid::from_parts(ms, 0);
            push("id", ">=", floor.to_string());
        }
        if !filter.actions.is_empty() {
            let list: Vec<String> = filter
                .actions
                .iter()
                .map(|a| format!("'{}'", a.as_str()))
                .collect();
            conditions.push(format!("action IN ({})", list.join(", ")));
        }
        let sql = format!(
            "{} WHERE {} ORDER BY id DESC LIMIT {}",
            ActivityRow::SELECT,
            conditions.join(" AND "),
            filter.limit
        );
        self.rows(&sql, params_from_iter(values), ActivityRow::read)?
            .into_iter()
            .map(ActivityRow::into_model)
            .collect()
    }

    fn activity_entry(&self, id: model::ActivityId) -> StoreResult<Option<ActivityEntry>> {
        let sql = format!("{} WHERE id = ?1", ActivityRow::SELECT);
        first(
            self.rows(&sql, [id.to_string()], ActivityRow::read)?,
            ActivityRow::into_model,
        )
    }

    fn claimed_tasks(&self) -> StoreResult<Vec<Task>> {
        let sql = format!(
            "{} WHERE claimed_by IS NOT NULL ORDER BY lease_until",
            TaskRow::SELECT
        );
        self.load_tasks(&sql, [])
    }

    fn inbox_item(&self, id: InboxId) -> StoreResult<Option<InboxItem>> {
        let sql = format!("{} WHERE id = ?1", InboxRow::SELECT);
        first(
            self.rows(&sql, [id.to_string()], InboxRow::read)?,
            InboxRow::into_model,
        )
    }

    fn inbox_items(&self, status: Option<InboxStatus>) -> StoreResult<Vec<InboxItem>> {
        let sql = format!(
            "{} WHERE ?1 IS NULL OR status = ?1 ORDER BY id DESC",
            InboxRow::SELECT
        );
        self.rows(&sql, [status.map(|s| s.as_str())], InboxRow::read)?
            .into_iter()
            .map(InboxRow::into_model)
            .collect()
    }

    fn epic(&self, id: model::EpicId) -> StoreResult<Option<model::Epic>> {
        let sql = format!("{} WHERE id = ?1", EpicRow::SELECT);
        first(
            self.rows(&sql, [id.to_string()], EpicRow::read)?,
            EpicRow::into_model,
        )
    }

    fn epics(&self, project: Option<ProjectId>) -> StoreResult<Vec<model::Epic>> {
        let sql = format!(
            "{} WHERE ?1 IS NULL OR project_id = ?1 ORDER BY target IS NULL, target, name",
            EpicRow::SELECT
        );
        self.rows(&sql, [project.map(|p| p.to_string())], EpicRow::read)?
            .into_iter()
            .map(EpicRow::into_model)
            .collect()
    }

    fn cycle(&self, id: model::CycleId) -> StoreResult<Option<model::Cycle>> {
        let sql = format!("{} WHERE id = ?1", CycleRow::SELECT);
        first(
            self.rows(&sql, [id.to_string()], CycleRow::read)?,
            CycleRow::into_model,
        )
    }

    fn cycles(&self, project: Option<ProjectId>) -> StoreResult<Vec<model::Cycle>> {
        let sql = format!(
            "{} WHERE ?1 IS NULL OR project_id = ?1 ORDER BY start_day, name",
            CycleRow::SELECT
        );
        self.rows(&sql, [project.map(|p| p.to_string())], CycleRow::read)?
            .into_iter()
            .map(CycleRow::into_model)
            .collect()
    }

    fn diagram(&self, id: model::DiagramId) -> StoreResult<Option<model::Diagram>> {
        let sql = format!("{} WHERE id = ?1", DiagramRow::SELECT);
        first(
            self.rows(&sql, [id.to_string()], DiagramRow::read)?,
            DiagramRow::into_model,
        )
    }

    fn diagram_by_number(
        &self,
        project: ProjectId,
        number: u64,
    ) -> StoreResult<Option<model::Diagram>> {
        let sql = format!(
            "{} WHERE project_id = ?1 AND number = ?2",
            DiagramRow::SELECT
        );
        first(
            self.rows(&sql, params![project.to_string(), number], DiagramRow::read)?,
            DiagramRow::into_model,
        )
    }

    fn diagrams(&self, project: Option<ProjectId>) -> StoreResult<Vec<model::Diagram>> {
        let sql = format!(
            "{} WHERE ?1 IS NULL OR project_id = ?1 ORDER BY project_id, number",
            DiagramRow::SELECT
        );
        self.rows(&sql, [project.map(|p| p.to_string())], DiagramRow::read)?
            .into_iter()
            .map(DiagramRow::into_model)
            .collect()
    }

    fn template(&self, id: model::TemplateId) -> StoreResult<Option<model::Template>> {
        let sql = format!("{} WHERE id = ?1", TemplateRow::SELECT);
        first(
            self.rows(&sql, [id.to_string()], TemplateRow::read)?,
            TemplateRow::into_model,
        )
    }

    fn template_by_name(&self, name: &str) -> StoreResult<Option<model::Template>> {
        let sql = format!("{} WHERE name = ?1", TemplateRow::SELECT);
        first(
            self.rows(&sql, [name.trim()], TemplateRow::read)?,
            TemplateRow::into_model,
        )
    }

    fn templates(&self) -> StoreResult<Vec<model::Template>> {
        let sql = format!("{} ORDER BY name", TemplateRow::SELECT);
        self.rows(&sql, [], TemplateRow::read)?
            .into_iter()
            .map(TemplateRow::into_model)
            .collect()
    }

    fn attachment(&self, id: model::AttachmentId) -> StoreResult<Option<model::Attachment>> {
        let sql = format!("{} WHERE id = ?1", AttachmentRow::SELECT);
        first(
            self.rows(&sql, [id.to_string()], AttachmentRow::read)?,
            AttachmentRow::into_model,
        )
    }

    fn attachments(&self, task: TaskId) -> StoreResult<Vec<model::Attachment>> {
        let sql = format!(
            "{} WHERE task_id = ?1 ORDER BY created_at, id",
            AttachmentRow::SELECT
        );
        self.rows(&sql, [task.to_string()], AttachmentRow::read)?
            .into_iter()
            .map(AttachmentRow::into_model)
            .collect()
    }

    fn attachment_data(&self, sha256: &str) -> StoreResult<Option<Vec<u8>>> {
        self.tx
            .query_row(
                "SELECT data FROM attachment_blob WHERE sha256 = ?1",
                [sha256],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)
    }

    fn relations(&self, task: Option<TaskId>) -> StoreResult<Vec<model::Relation>> {
        let rows = self.rows(
            "SELECT a_id, b_id, kind FROM task_relation
             WHERE ?1 IS NULL OR a_id = ?1 OR b_id = ?1 ORDER BY a_id, b_id",
            [task.map(|t| t.to_string())],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )?;
        rows.into_iter()
            .map(|(a, b, kind)| {
                Ok(model::Relation {
                    a: parse("task_relation.a_id", &a)?,
                    b: parse("task_relation.b_id", &b)?,
                    kind: parse("task_relation.kind", &kind)?,
                })
            })
            .collect()
    }

    fn mentions_of(&self, target: TaskId) -> StoreResult<Vec<model::Mention>> {
        let rows = self.rows(
            "SELECT source_id, comment_id FROM mention WHERE target_id = ?1 ORDER BY source_id",
            [target.to_string()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )?;
        rows.into_iter()
            .map(|(source, comment)| {
                Ok(model::Mention {
                    source: parse("mention.source_id", &source)?,
                    comment: comment
                        .map(|c| parse("mention.comment_id", &c))
                        .transpose()?,
                    target,
                })
            })
            .collect()
    }

    fn diagram_mentions_of(
        &self,
        target: model::DiagramId,
    ) -> StoreResult<Vec<model::DiagramMention>> {
        self.diagram_mentions_where("target_id", target.to_string())
    }

    fn diagram_mentions_from(&self, source: TaskId) -> StoreResult<Vec<model::DiagramMention>> {
        self.diagram_mentions_where("source_id", source.to_string())
    }

    fn automations(&self) -> StoreResult<Vec<model::Automation>> {
        let rows = self.rows(
            "SELECT id, name, trigger, condition, command, args, enabled, project_id,
                    created_at, updated_at
             FROM automation ORDER BY name COLLATE NOCASE",
            [],
            |r| {
                Ok((
                    (
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                    ),
                    (
                        r.get::<_, String>(5)?,
                        r.get::<_, bool>(6)?,
                        r.get::<_, Option<String>>(7)?,
                        r.get::<_, String>(8)?,
                        r.get::<_, String>(9)?,
                    ),
                ))
            },
        )?;
        rows.into_iter()
            .map(
                |(
                    (id, name, trigger, condition, command),
                    (args, enabled, project, created, updated),
                )| {
                    Ok(model::Automation {
                        id: parse("automation.id", &id)?,
                        name,
                        trigger: parse("automation.trigger", &trigger)?,
                        condition,
                        command,
                        args: json("automation.args", &args)?,
                        enabled,
                        project_id: project
                            .map(|p| parse("automation.project_id", &p))
                            .transpose()?,
                        created_at: parse("automation.created_at", &created)?,
                        updated_at: parse("automation.updated_at", &updated)?,
                    })
                },
            )
            .collect()
    }

    fn webhooks(&self) -> StoreResult<Vec<model::Webhook>> {
        let rows = self.rows(
            "SELECT id, url, secret, triggers, enabled, created_at FROM webhook ORDER BY created_at",
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, bool>(4)?,
                    r.get::<_, String>(5)?,
                ))
            },
        )?;
        rows.into_iter()
            .map(|(id, url, secret, triggers, enabled, created)| {
                Ok(model::Webhook {
                    id: parse("webhook.id", &id)?,
                    url,
                    secret,
                    triggers: json("webhook.triggers", &triggers)?,
                    enabled,
                    created_at: parse("webhook.created_at", &created)?,
                })
            })
            .collect()
    }

    fn deliveries_due(
        &self,
        now: model::Timestamp,
        limit: usize,
    ) -> StoreResult<Vec<model::Delivery>> {
        let sql = format!(
            "{DELIVERY_SELECT} WHERE status = 'pending' AND next_at <= ?1 ORDER BY next_at, id LIMIT ?2"
        );
        self.rows(&sql, params![now.to_string(), limit], delivery_row)?
            .into_iter()
            .collect()
    }

    fn delivery(&self, id: model::DeliveryId) -> StoreResult<Option<model::Delivery>> {
        let sql = format!("{DELIVERY_SELECT} WHERE id = ?1");
        self.rows(&sql, [id.to_string()], delivery_row)?
            .into_iter()
            .next()
            .transpose()
    }

    fn deliveries(
        &self,
        webhook: Option<model::WebhookId>,
        limit: usize,
    ) -> StoreResult<Vec<model::Delivery>> {
        let sql = format!(
            "{DELIVERY_SELECT} WHERE ?1 IS NULL OR webhook_id = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2"
        );
        self.rows(
            &sql,
            params![webhook.map(|w| w.to_string()), limit],
            delivery_row,
        )?
        .into_iter()
        .collect()
    }

    fn views(&self) -> StoreResult<Vec<model::SavedView>> {
        let rows = self.rows(
            "SELECT id, name, filter, layout, columns, project_id, created_at, updated_at
             FROM saved_view ORDER BY name COLLATE NOCASE",
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ))
            },
        )?;
        rows.into_iter()
            .map(
                |(id, name, filter, layout, columns, project, created, updated)| {
                    Ok(model::SavedView {
                        id: parse("saved_view.id", &id)?,
                        name,
                        filter,
                        layout,
                        columns: json("saved_view.columns", &columns)?,
                        project_id: project
                            .map(|p| parse("saved_view.project_id", &p))
                            .transpose()?,
                        created_at: parse("saved_view.created_at", &created)?,
                        updated_at: parse("saved_view.updated_at", &updated)?,
                    })
                },
            )
            .collect()
    }

    fn today_marks(&self) -> StoreResult<HashMap<TaskId, model::Date>> {
        self.rows("SELECT task_id, day FROM today_mark", [], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .into_iter()
        .map(|(task, day)| {
            Ok((
                parse("today_mark.task_id", &task)?,
                parse("today_mark.day", &day)?,
            ))
        })
        .collect()
    }

    fn time_entries(&self, filter: &store::TimeFilter) -> StoreResult<Vec<model::TimeEntry>> {
        let mut conditions = vec!["1 = 1".to_owned()];
        let mut values: Vec<String> = Vec::new();
        let mut push = |cond: &str, value: String| {
            values.push(value);
            conditions.push(cond.replace('?', &format!("?{}", values.len())));
        };
        if let Some(task) = filter.task {
            push("task_id = ?", task.to_string());
        }
        if let Some(actor) = &filter.actor {
            push("actor = ?", actor.to_string());
        }
        if let Some(since) = filter.since {
            push("started_at >= ?", since.to_string());
        }
        if let Some(until) = filter.until {
            push("started_at < ?", until.to_string());
        }
        if filter.running {
            conditions.push("running = 1".into());
        }
        let sql = format!(
            "{} WHERE {} ORDER BY started_at, id",
            TimeRow::SELECT,
            conditions.join(" AND ")
        );
        self.rows(&sql, params_from_iter(values), TimeRow::read)?
            .into_iter()
            .map(TimeRow::into_model)
            .collect()
    }

    fn cost_entries(&self, filter: &store::CostFilter) -> StoreResult<Vec<model::CostEntry>> {
        let mut conditions = vec!["1 = 1".to_owned()];
        let mut values: Vec<String> = Vec::new();
        let mut push = |cond: &str, value: String| {
            values.push(value);
            conditions.push(cond.replace('?', &format!("?{}", values.len())));
        };
        if let Some(task) = filter.task {
            push("task_id = ?", task.to_string());
        }
        if let Some(project) = filter.project {
            push(
                "task_id IN (SELECT id FROM task WHERE project_id = ?)",
                project.to_string(),
            );
        }
        if let Some(actor) = &filter.actor {
            push("actor = ?", actor.to_string());
        }
        if let Some(since) = filter.since {
            push("at >= ?", since.to_string());
        }
        if let Some(until) = filter.until {
            push("at < ?", until.to_string());
        }
        let sql = format!(
            "{} WHERE {} ORDER BY at, id",
            CostRow::SELECT,
            conditions.join(" AND ")
        );
        self.rows(&sql, params_from_iter(values), CostRow::read)?
            .into_iter()
            .map(CostRow::into_model)
            .collect()
    }

    fn git_links(&self, task: Option<TaskId>) -> StoreResult<Vec<model::GitLink>> {
        let rows = match task {
            Some(t) => self.rows(
                &format!("{} WHERE task_id = ?1 ORDER BY at, id", GitRow::SELECT),
                [t.to_string()],
                GitRow::read,
            )?,
            None => self.rows(
                &format!("{} ORDER BY at, id", GitRow::SELECT),
                [],
                GitRow::read,
            )?,
        };
        rows.into_iter().map(GitRow::into_model).collect()
    }
}

/// Primera fila convertida al modelo, si la hay.
fn first<R, T>(rows: Vec<R>, convert: impl FnOnce(R) -> StoreResult<T>) -> StoreResult<Option<T>> {
    rows.into_iter().next().map(convert).transpose()
}

/// Devuelve un `Result` dentro del `Ok` de rusqlite para separar los errores
/// de SQL de los de datos corruptos.
fn read_label(r: &Row<'_>) -> rusqlite::Result<StoreResult<Label>> {
    let id: String = r.get("id")?;
    let name: String = r.get("name")?;
    let color: String = r.get("color")?;
    Ok(parse("label.id", &id).map(|id| Label { id, name, color }))
}

impl Writer for SqliteTx<'_> {
    fn update_workspace(&mut self, w: &WorkspaceInfo) -> StoreResult<()> {
        let st = &w.settings;
        self.tx
            .execute(
                "UPDATE workspace SET name = ?1, max_depth = ?2, trash_days = ?3, lease_minutes = ?4,
                        agent_wip_limit = ?5, auto_archive_days = ?6, max_attachment_mb = ?7,
                        read_only = ?9, backup_hours = ?10, backup_keep = ?11,
                        stale_weeks = ?12, urgent_days = ?13
                 WHERE id = ?8",
                params![
                    w.name,
                    st.max_depth,
                    st.trash_days,
                    st.lease_minutes,
                    st.agent_wip_limit,
                    st.auto_archive_days,
                    st.max_attachment_mb,
                    w.id.to_string(),
                    st.read_only,
                    st.backup_hours,
                    st.backup_keep,
                    st.stale_weeks,
                    st.urgent_days,
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn insert_project(&mut self, p: &Project) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT INTO project (id, key, name, description, status,
                                      next_number, created_at, updated_at, locked)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    p.id.to_string(),
                    p.key,
                    p.name,
                    p.description,
                    p.status.as_str(),
                    p.next_number,
                    p.created_at.to_string(),
                    p.updated_at.to_string(),
                    p.locked,
                ],
            )
            .map_err(db)?;
        self.write_project_extra(p)
    }

    fn update_project(&mut self, p: &Project) -> StoreResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE project SET key = ?2, name = ?3, description = ?4, status = ?5,
                        next_number = ?6, updated_at = ?7, locked = ?8
                 WHERE id = ?1",
                params![
                    p.id.to_string(),
                    p.key,
                    p.name,
                    p.description,
                    p.status.as_str(),
                    p.next_number,
                    p.updated_at.to_string(),
                    p.locked,
                ],
            )
            .map_err(db)?;
        expect_one(changed, "project", p.id)?;
        self.write_project_extra(p)
    }

    fn insert_task(&mut self, t: &Task) -> StoreResult<()> {
        let claim = ClaimCols::of(t);
        self.tx
            .execute(
                "INSERT INTO task (id, project_id, number, parent_id, title, description, status,
                                   priority, start_at, due_at, assignee, creator, revision,
                                   created_at, updated_at, archived_at, deleted_at, claimed_by,
                                   claimed_at, lease_until, claim_revision, needs_human, locked)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                         ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
                params![
                    t.id.to_string(),
                    t.project_id.to_string(),
                    t.number,
                    text(&t.parent_id),
                    t.title,
                    t.description,
                    t.status.as_str(),
                    t.priority.as_str(),
                    text(&t.start),
                    text(&t.due),
                    t.assignee().map(ToString::to_string),
                    t.creator.to_string(),
                    t.revision,
                    t.created_at.to_string(),
                    t.updated_at.to_string(),
                    text(&t.archived_at),
                    text(&t.deleted_at),
                    claim.agent,
                    claim.since,
                    claim.until,
                    claim.revision,
                    t.needs_human,
                    t.locked,
                ],
            )
            .map_err(db)?;
        self.write_task_parts(t)?;
        self.write_task_extra(t)
    }

    fn update_task(&mut self, t: &Task) -> StoreResult<()> {
        let claim = ClaimCols::of(t);
        let changed = self
            .tx
            .execute(
                "UPDATE task SET parent_id = ?2, title = ?3, description = ?4, status = ?5,
                        priority = ?6, start_at = ?7, due_at = ?8, assignee = ?9, revision = ?10,
                        updated_at = ?11, archived_at = ?12, deleted_at = ?13, claimed_by = ?14,
                        claimed_at = ?15, lease_until = ?16, claim_revision = ?17, needs_human = ?18,
                        locked = ?19
                 WHERE id = ?1",
                params![
                    t.id.to_string(),
                    text(&t.parent_id),
                    t.title,
                    t.description,
                    t.status.as_str(),
                    t.priority.as_str(),
                    text(&t.start),
                    text(&t.due),
                    t.assignee().map(ToString::to_string),
                    t.revision,
                    t.updated_at.to_string(),
                    text(&t.archived_at),
                    text(&t.deleted_at),
                    claim.agent,
                    claim.since,
                    claim.until,
                    claim.revision,
                    t.needs_human,
                    t.locked,
                ],
            )
            .map_err(db)?;
        expect_one(changed, "task", t.id)?;
        self.write_task_parts(t)?;
        self.write_task_extra(t)
    }

    fn purge_task(&mut self, id: TaskId) -> StoreResult<()> {
        self.tx
            .execute("DELETE FROM task WHERE id = ?1", [id.to_string()])
            .map_err(db)?;
        Ok(())
    }

    fn delete_project(&mut self, id: ProjectId) -> StoreResult<()> {
        let id = id.to_string();
        self.with_activity_gate(|tx| {
            tx.execute("DELETE FROM activity WHERE project_id = ?1", [&id])?;
            Ok(())
        })?;
        for sql in [
            "DELETE FROM task WHERE project_id = ?1",
            "DELETE FROM epic WHERE project_id = ?1",
            "DELETE FROM saved_view WHERE project_id = ?1",
            "DELETE FROM automation WHERE project_id = ?1",
            "DELETE FROM cycle WHERE project_id = ?1",
            "DELETE FROM diagram WHERE project_id = ?1",
            "DELETE FROM project WHERE id = ?1",
        ] {
            self.tx.execute(sql, [&id]).map_err(db)?;
        }
        self.tx
            .execute(
                "DELETE FROM attachment_blob WHERE sha256 NOT IN (SELECT sha256 FROM attachment)",
                [],
            )
            .map_err(db)?;
        Ok(())
    }

    fn wipe_workspace(&mut self) -> StoreResult<()> {
        self.with_activity_gate(|tx| {
            tx.execute("DELETE FROM activity", [])?;
            Ok(())
        })?;
        // En orden: lo que apunta a otra tabla, antes que ella.
        for table in [
            "webhook_delivery",
            "webhook",
            "automation",
            "saved_view",
            "today_mark",
            "time_entry",
            "cost_entry",
            "git_link",
            "attachment",
            "attachment_blob",
            "task_item",
            "task_label",
            "task_relation",
            "task_dependency",
            "mention",
            "diagram_mention",
            "comment",
            "task",
            "diagram",
            "cycle",
            "epic",
            "template",
            "inbox_item",
            "label",
            "agent_session",
            "agent",
            "interface",
            "project",
            "idempotency",
        ] {
            self.tx
                .execute(&format!("DELETE FROM {table}"), [])
                .map_err(db)?;
        }
        Ok(())
    }

    fn prune_activity(&mut self, before: model::Timestamp) -> StoreResult<usize> {
        let before = before.to_string();
        self.with_activity_gate(|tx| tx.execute("DELETE FROM activity WHERE at < ?1", [&before]))
    }

    fn insert_label(&mut self, l: &Label) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT INTO label (id, name, color) VALUES (?1, ?2, ?3)",
                params![l.id.to_string(), l.name, l.color],
            )
            .map_err(db)?;
        Ok(())
    }

    fn update_label(&mut self, l: &Label) -> StoreResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE label SET name = ?2, color = ?3 WHERE id = ?1",
                params![l.id.to_string(), l.name, l.color],
            )
            .map_err(db)?;
        expect_one(changed, "label", l.id)
    }

    fn add_dependency(&mut self, d: Dependency) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT OR IGNORE INTO task_dependency (blocker_id, blocked_id) VALUES (?1, ?2)",
                [d.blocker.to_string(), d.blocked.to_string()],
            )
            .map_err(db)?;
        Ok(())
    }

    fn remove_dependency(&mut self, d: Dependency) -> StoreResult<bool> {
        let changed = self
            .tx
            .execute(
                "DELETE FROM task_dependency WHERE blocker_id = ?1 AND blocked_id = ?2",
                [d.blocker.to_string(), d.blocked.to_string()],
            )
            .map_err(db)?;
        Ok(changed > 0)
    }

    fn insert_interface(&mut self, i: &model::Interface) -> StoreResult<()> {
        self.exec_interface(
            "INSERT INTO interface (id, name, kind, description, color, status, level, scope,
                                    max_agents, paused, token_hash, created_at, updated_at, token)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            i,
        )?;
        Ok(())
    }

    fn update_interface(&mut self, i: &model::Interface) -> StoreResult<()> {
        let changed = self.exec_interface(
            "UPDATE interface SET name = ?2, kind = ?3, description = ?4, color = ?5, status = ?6,
                    level = ?7, scope = ?8, max_agents = ?9, paused = ?10, token_hash = ?11,
                    updated_at = ?13, token = ?14
             WHERE id = ?1 AND created_at = ?12",
            i,
        )?;
        expect_one(changed, "interface", i.id)
    }

    fn insert_agent(&mut self, a: &Agent) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT INTO agent (id, name, model, description, color, status, created_at,
                                    updated_at, interface_id, level, scope, capabilities, paused)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    a.id.to_string(),
                    a.name,
                    a.model,
                    a.description,
                    a.color,
                    a.status.as_str(),
                    a.created_at.to_string(),
                    a.updated_at.to_string(),
                    text(&a.interface_id),
                    a.level.as_str(),
                    to_json(&a.scope),
                    to_json(&a.capabilities),
                    a.paused,
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn update_agent(&mut self, a: &Agent) -> StoreResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE agent SET model = ?2, description = ?3, color = ?4, status = ?5,
                        updated_at = ?6, interface_id = ?7, level = ?8, scope = ?9,
                        capabilities = ?10, paused = ?11
                 WHERE id = ?1",
                params![
                    a.id.to_string(),
                    a.model,
                    a.description,
                    a.color,
                    a.status.as_str(),
                    a.updated_at.to_string(),
                    text(&a.interface_id),
                    a.level.as_str(),
                    to_json(&a.scope),
                    to_json(&a.capabilities),
                    a.paused,
                ],
            )
            .map_err(db)?;
        expect_one(changed, "agent", a.id)
    }

    fn upsert_session(&mut self, s: &AgentSession) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT INTO agent_session (id, agent_id, transport, started_at, last_seen, ended_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (id) DO UPDATE SET last_seen = excluded.last_seen,
                                                ended_at = excluded.ended_at",
                params![
                    s.id.to_string(),
                    s.agent_id.to_string(),
                    s.transport,
                    s.started_at.to_string(),
                    s.last_seen.to_string(),
                    text(&s.ended_at),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn insert_comment(&mut self, c: &Comment) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT INTO comment (id, task_id, author, body, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    c.id.to_string(),
                    c.task_id.to_string(),
                    c.author.to_string(),
                    c.body,
                    c.created_at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn insert_inbox_item(&mut self, i: &InboxItem) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT INTO inbox_item (id, kind, title, body, author, status, task_id,
                                         created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    i.id.to_string(),
                    i.kind.as_str(),
                    i.title,
                    i.body,
                    i.author.to_string(),
                    i.status.as_str(),
                    text(&i.task_id),
                    i.created_at.to_string(),
                    i.updated_at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn update_inbox_item(&mut self, i: &InboxItem) -> StoreResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE inbox_item SET status = ?2, task_id = ?3, updated_at = ?4 WHERE id = ?1",
                params![
                    i.id.to_string(),
                    i.status.as_str(),
                    text(&i.task_id),
                    i.updated_at.to_string(),
                ],
            )
            .map_err(db)?;
        expect_one(changed, "inbox item", i.id)
    }

    fn insert_epic(&mut self, e: &model::Epic) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT INTO epic (id, project_id, kind, name, description, target, status, locked,
                                   created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    e.id.to_string(),
                    e.project_id.to_string(),
                    e.kind.as_str(),
                    e.name,
                    e.description,
                    text(&e.target),
                    e.status.as_str(),
                    e.locked,
                    e.created_at.to_string(),
                    e.updated_at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn update_epic(&mut self, e: &model::Epic) -> StoreResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE epic SET name = ?2, description = ?3, target = ?4, status = ?5, locked = ?6,
                        updated_at = ?7
                 WHERE id = ?1",
                params![
                    e.id.to_string(),
                    e.name,
                    e.description,
                    text(&e.target),
                    e.status.as_str(),
                    e.locked,
                    e.updated_at.to_string(),
                ],
            )
            .map_err(db)?;
        expect_one(changed, "epic", e.id)
    }

    fn delete_epic(&mut self, id: model::EpicId) -> StoreResult<()> {
        let id = id.to_string();
        self.tx
            .execute("UPDATE task SET epic_id = NULL WHERE epic_id = ?1", [&id])
            .map_err(db)?;
        self.tx
            .execute(
                "UPDATE task SET milestone_id = NULL WHERE milestone_id = ?1",
                [&id],
            )
            .map_err(db)?;
        self.tx
            .execute("DELETE FROM epic WHERE id = ?1", [&id])
            .map_err(db)?;
        Ok(())
    }

    fn insert_cycle(&mut self, c: &model::Cycle) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT INTO cycle (id, project_id, name, start_day, end_day, closed_at,
                                    created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    c.id.to_string(),
                    c.project_id.to_string(),
                    c.name,
                    c.start.to_string(),
                    c.end.to_string(),
                    text(&c.closed_at),
                    c.created_at.to_string(),
                    c.updated_at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn update_cycle(&mut self, c: &model::Cycle) -> StoreResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE cycle SET name = ?2, start_day = ?3, end_day = ?4, closed_at = ?5,
                        updated_at = ?6
                 WHERE id = ?1",
                params![
                    c.id.to_string(),
                    c.name,
                    c.start.to_string(),
                    c.end.to_string(),
                    text(&c.closed_at),
                    c.updated_at.to_string(),
                ],
            )
            .map_err(db)?;
        expect_one(changed, "cycle", c.id)
    }

    fn delete_cycle(&mut self, id: model::CycleId) -> StoreResult<()> {
        let id = id.to_string();
        self.tx
            .execute(
                "UPDATE task SET cycles =
                     (SELECT json_group_array(value) FROM json_each(task.cycles) WHERE value <> ?1)
                 WHERE EXISTS (SELECT 1 FROM json_each(task.cycles) WHERE value = ?1)",
                [&id],
            )
            .map_err(db)?;
        self.tx
            .execute("DELETE FROM cycle WHERE id = ?1", [&id])
            .map_err(db)?;
        Ok(())
    }

    fn insert_diagram(&mut self, d: &model::Diagram) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT INTO diagram (id, project_id, number, title, description, source,
                                      created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    d.id.to_string(),
                    d.project_id.to_string(),
                    d.number,
                    d.title,
                    d.description,
                    d.source,
                    d.created_at.to_string(),
                    d.updated_at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn update_diagram(&mut self, d: &model::Diagram) -> StoreResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE diagram SET title = ?2, description = ?3, source = ?4, updated_at = ?5
                 WHERE id = ?1",
                params![
                    d.id.to_string(),
                    d.title,
                    d.description,
                    d.source,
                    d.updated_at.to_string(),
                ],
            )
            .map_err(db)?;
        expect_one(changed, "diagram", d.id)
    }

    fn delete_diagram(&mut self, id: model::DiagramId) -> StoreResult<()> {
        let id = id.to_string();
        self.tx
            .execute("DELETE FROM diagram_mention WHERE target_id = ?1", [&id])
            .map_err(db)?;
        self.tx
            .execute("DELETE FROM diagram WHERE id = ?1", [&id])
            .map_err(db)?;
        Ok(())
    }

    fn insert_template(&mut self, t: &model::Template) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT INTO template (id, name, kind, description, body, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    t.id.to_string(),
                    t.name,
                    t.kind.as_str(),
                    t.description,
                    t.body,
                    t.created_at.to_string(),
                    t.updated_at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn update_template(&mut self, t: &model::Template) -> StoreResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE template SET name = ?2, description = ?3, body = ?4, updated_at = ?5
                 WHERE id = ?1",
                params![
                    t.id.to_string(),
                    t.name,
                    t.description,
                    t.body,
                    t.updated_at.to_string(),
                ],
            )
            .map_err(db)?;
        expect_one(changed, "template", t.id)
    }

    fn delete_template(&mut self, id: model::TemplateId) -> StoreResult<()> {
        self.tx
            .execute("DELETE FROM template WHERE id = ?1", [id.to_string()])
            .map_err(db)?;
        Ok(())
    }

    fn insert_attachment(&mut self, a: &model::Attachment, data: &[u8]) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT OR IGNORE INTO attachment_blob (sha256, data) VALUES (?1, ?2)",
                params![a.sha256, data],
            )
            .map_err(db)?;
        self.tx
            .execute(
                "INSERT INTO attachment (id, task_id, name, mime, size, sha256, author, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    a.id.to_string(),
                    a.task_id.to_string(),
                    a.name,
                    a.mime,
                    a.size,
                    a.sha256,
                    a.author.to_string(),
                    a.created_at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn delete_attachment(&mut self, id: model::AttachmentId) -> StoreResult<()> {
        self.tx
            .execute("DELETE FROM attachment WHERE id = ?1", [id.to_string()])
            .map_err(db)?;
        // El contenido se borra cuando ya no lo adjunta nadie.
        self.tx
            .execute(
                "DELETE FROM attachment_blob
                 WHERE sha256 NOT IN (SELECT sha256 FROM attachment)",
                [],
            )
            .map_err(db)?;
        Ok(())
    }

    fn add_relation(&mut self, r: model::Relation) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT OR IGNORE INTO task_relation (a_id, b_id, kind) VALUES (?1, ?2, ?3)",
                params![r.a.to_string(), r.b.to_string(), r.kind.as_str()],
            )
            .map_err(db)?;
        Ok(())
    }

    fn remove_relation(&mut self, r: model::Relation) -> StoreResult<bool> {
        let changed = self
            .tx
            .execute(
                "DELETE FROM task_relation WHERE a_id = ?1 AND b_id = ?2 AND kind = ?3",
                params![r.a.to_string(), r.b.to_string(), r.kind.as_str()],
            )
            .map_err(db)?;
        Ok(changed > 0)
    }

    fn set_mentions(
        &mut self,
        source: TaskId,
        comment: Option<model::CommentId>,
        targets: &[TaskId],
    ) -> StoreResult<()> {
        let comment = text(&comment);
        self.tx
            .execute(
                "DELETE FROM mention WHERE source_id = ?1 AND comment_id IS ?2",
                params![source.to_string(), comment],
            )
            .map_err(db)?;
        let mut insert = self
            .tx
            .prepare_cached(
                "INSERT INTO mention (source_id, comment_id, target_id) VALUES (?1, ?2, ?3)",
            )
            .map_err(db)?;
        for target in targets {
            insert
                .execute(params![source.to_string(), comment, target.to_string()])
                .map_err(db)?;
        }
        Ok(())
    }

    fn set_diagram_mentions(
        &mut self,
        source: TaskId,
        comment: Option<model::CommentId>,
        targets: &[model::DiagramId],
    ) -> StoreResult<()> {
        let comment = text(&comment);
        self.tx
            .execute(
                "DELETE FROM diagram_mention WHERE source_id = ?1 AND comment_id IS ?2",
                params![source.to_string(), comment],
            )
            .map_err(db)?;
        let mut insert = self
            .tx
            .prepare_cached(
                "INSERT INTO diagram_mention (source_id, comment_id, target_id)
                 VALUES (?1, ?2, ?3)",
            )
            .map_err(db)?;
        for target in targets {
            insert
                .execute(params![source.to_string(), comment, target.to_string()])
                .map_err(db)?;
        }
        Ok(())
    }

    fn save_automation(&mut self, a: &model::Automation) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT OR REPLACE INTO automation (id, name, trigger, condition, command, args,
                                                    enabled, project_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    a.id.to_string(),
                    a.name,
                    a.trigger.as_str(),
                    a.condition,
                    a.command,
                    to_json(&a.args),
                    a.enabled,
                    a.project_id.map(|p| p.to_string()),
                    a.created_at.to_string(),
                    a.updated_at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn delete_automation(&mut self, id: model::AutomationId) -> StoreResult<()> {
        self.tx
            .execute("DELETE FROM automation WHERE id = ?1", [id.to_string()])
            .map_err(db)?;
        Ok(())
    }

    fn save_webhook(&mut self, w: &model::Webhook) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT OR REPLACE INTO webhook (id, url, secret, triggers, enabled, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    w.id.to_string(),
                    w.url,
                    w.secret,
                    to_json(&w.triggers),
                    w.enabled,
                    w.created_at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn delete_webhook(&mut self, id: model::WebhookId) -> StoreResult<()> {
        self.tx
            .execute("DELETE FROM webhook WHERE id = ?1", [id.to_string()])
            .map_err(db)?;
        Ok(())
    }

    fn save_delivery(&mut self, d: &model::Delivery) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT OR REPLACE INTO webhook_delivery (id, webhook_id, trigger, payload, attempts,
                                                          next_at, status, last_error, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    d.id.to_string(),
                    d.webhook_id.to_string(),
                    d.trigger.as_str(),
                    d.payload,
                    d.attempts,
                    d.next_at.to_string(),
                    d.status.as_str(),
                    d.last_error,
                    d.created_at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn save_view(&mut self, v: &model::SavedView) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT OR REPLACE INTO saved_view (id, name, filter, layout, columns, project_id,
                                                    created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    v.id.to_string(),
                    v.name,
                    v.filter,
                    v.layout,
                    to_json(&v.columns),
                    v.project_id.map(|p| p.to_string()),
                    v.created_at.to_string(),
                    v.updated_at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn delete_view(&mut self, id: model::ViewId) -> StoreResult<()> {
        self.tx
            .execute("DELETE FROM saved_view WHERE id = ?1", [id.to_string()])
            .map_err(db)?;
        Ok(())
    }

    fn set_today_mark(&mut self, task: TaskId, day: Option<model::Date>) -> StoreResult<()> {
        match day {
            Some(day) => self.tx.execute(
                "INSERT OR REPLACE INTO today_mark (task_id, day) VALUES (?1, ?2)",
                params![task.to_string(), day.to_string()],
            ),
            None => self.tx.execute(
                "DELETE FROM today_mark WHERE task_id = ?1",
                [task.to_string()],
            ),
        }
        .map_err(db)?;
        Ok(())
    }

    fn insert_time_entry(&mut self, e: &model::TimeEntry) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT INTO time_entry (id, task_id, actor, source, started_at, minutes, running,
                                         note, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    e.id.to_string(),
                    e.task_id.to_string(),
                    e.actor.to_string(),
                    e.source.as_str(),
                    e.started_at.to_string(),
                    e.minutes,
                    e.running,
                    e.note,
                    e.created_at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn update_time_entry(&mut self, e: &model::TimeEntry) -> StoreResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE time_entry SET minutes = ?2, running = ?3, note = ?4 WHERE id = ?1",
                params![e.id.to_string(), e.minutes, e.running, e.note],
            )
            .map_err(db)?;
        expect_one(changed, "time entry", e.id)
    }

    fn insert_cost_entry(&mut self, e: &model::CostEntry) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT INTO cost_entry (id, task_id, actor, model, tokens_in, tokens_out,
                                         amount_micros, currency, note, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    e.id.to_string(),
                    e.task_id.to_string(),
                    e.actor.to_string(),
                    e.model,
                    e.tokens_in,
                    e.tokens_out,
                    e.amount_micros,
                    e.currency,
                    e.note,
                    e.at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn insert_git_link(&mut self, l: &model::GitLink) -> StoreResult<bool> {
        let inserted = self
            .tx
            .execute(
                "INSERT INTO git_link (id, task_id, kind, reference, title, url, actor, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (task_id, kind, reference) DO NOTHING",
                params![
                    l.id.to_string(),
                    l.task_id.to_string(),
                    l.kind.as_str(),
                    l.reference,
                    l.title,
                    l.url,
                    l.actor.to_string(),
                    l.at.to_string(),
                ],
            )
            .map_err(db)?;
        Ok(inserted == 1)
    }

    fn delete_git_link(&mut self, id: model::GitLinkId) -> StoreResult<bool> {
        let deleted = self
            .tx
            .execute("DELETE FROM git_link WHERE id = ?1", [id.to_string()])
            .map_err(db)?;
        Ok(deleted == 1)
    }

    fn save_idempotent(
        &mut self,
        actor: &model::Actor,
        key: &str,
        command: &str,
        response: &str,
        at: model::Timestamp,
    ) -> StoreResult<()> {
        self.tx
            .execute(
                "INSERT OR REPLACE INTO idempotency (actor, key, command, response, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![actor.to_string(), key, command, response, at.to_string()],
            )
            .map_err(db)?;
        Ok(())
    }

    fn purge_idempotency(&mut self, before: model::Timestamp) -> StoreResult<usize> {
        self.tx
            .execute(
                "DELETE FROM idempotency WHERE created_at < ?1",
                [before.to_string()],
            )
            .map_err(db)
    }
}

impl Transaction for SqliteTx<'_> {
    fn append_activity(&mut self, e: &ActivityEntry) -> StoreResult<()> {
        let changes = serde_json::to_string(&e.changes)
            .map_err(|err| StoreError::Backend(format!("activity.changes: {err}")))?;
        self.tx
            .execute(
                "INSERT INTO activity (id, at, actor, correlation, command, action, entity_kind,
                                       entity_id, project_id, task_id, changes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    e.id.to_string(),
                    e.at.to_string(),
                    e.actor.to_string(),
                    e.correlation.to_string(),
                    e.command,
                    e.action.as_str(),
                    e.entity.as_str(),
                    e.entity_id,
                    text(&e.project_id),
                    text(&e.task_id),
                    changes,
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    fn commit(self: Box<Self>) -> StoreResult<()> {
        let Self { tx, commits } = *self;
        tx.commit().map_err(db)?;
        if let Some(commits) = commits {
            *commits = commits.wrapping_add(1);
        }
        Ok(())
    }
}

fn expect_one(changed: usize, what: &str, id: impl std::fmt::Display) -> StoreResult<()> {
    if changed == 1 {
        Ok(())
    } else {
        Err(StoreError::Backend(format!("{what} {id} does not exist")))
    }
}

/// Columnas del claim: las tres llenas o las tres vacías.
struct ClaimCols {
    agent: Option<String>,
    since: Option<String>,
    until: Option<String>,
    revision: Option<u64>,
}

impl ClaimCols {
    fn of(t: &Task) -> Self {
        Self {
            agent: t.claim.map(|c| c.agent.to_string()),
            since: t.claim.map(|c| c.since.to_string()),
            until: t.claim.map(|c| c.until.to_string()),
            revision: t.claim.map(|c| c.revision),
        }
    }
}

/// Texto libre → consulta FTS5: cada palabra entre comillas (sin operadores
/// del usuario) y como prefijo, todas obligatorias. `None` si no queda nada.
fn fts_query(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

/// IDs por consulta al leer partes de tareas en lote (por debajo del límite
/// de variables de SQLite).
const PARTS_CHUNK: usize = 900;

/// `'done', 'cancelled'`: lista SQL de estados canónicos (texto fijo, sin
/// datos del usuario). Vacía → una que no coincide con nada.
fn status_list(statuses: &[Status]) -> String {
    if statuses.is_empty() {
        return "''".into();
    }
    statuses
        .iter()
        .map(|s| format!("'{}'", s.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn count(n: i64) -> usize {
    usize::try_from(n).unwrap_or(0)
}

const DELIVERY_SELECT: &str = "SELECT id, webhook_id, trigger, payload, attempts, next_at, status, \
                               last_error, created_at FROM webhook_delivery";

fn delivery_row(r: &Row<'_>) -> rusqlite::Result<StoreResult<model::Delivery>> {
    let text = |i: usize| r.get::<_, String>(i);
    let (id, webhook, trigger, payload) = (text(0)?, text(1)?, text(2)?, text(3)?);
    let attempts: u32 = r.get(4)?;
    let (next_at, status, last_error, created) = (text(5)?, text(6)?, text(7)?, text(8)?);
    Ok((|| {
        Ok(model::Delivery {
            id: parse("webhook_delivery.id", &id)?,
            webhook_id: parse("webhook_delivery.webhook_id", &webhook)?,
            trigger: parse("webhook_delivery.trigger", &trigger)?,
            payload,
            attempts,
            next_at: parse("webhook_delivery.next_at", &next_at)?,
            status: parse("webhook_delivery.status", &status)?,
            last_error,
            created_at: parse("webhook_delivery.created_at", &created)?,
        })
    })())
}
