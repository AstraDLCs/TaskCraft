//! Lenguaje de filtros (D12, RF-VIS-12): una sola gramática para la app, la
//! CLI y el MCP. `status:todo priority>=high -label:docs due<=today+3d`.

use std::collections::{BTreeMap, HashMap};

use jiff::civil::Date;
use jiff::tz::TimeZone;
use model::{Actor, AgentId, EpicId, LabelId, Priority, ProjectId, Status, Task, Timestamp};

/// Lo que se sabe de una tarea para filtrarla: la arma quien tiene los
/// datos (el engine o la app), con nombres ya resueltos.
#[derive(Debug, Clone)]
pub struct Facts<'a> {
    pub key: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub status: Status,
    pub stage: Option<&'a str>,
    pub priority: Priority,
    pub labels: Vec<&'a str>,
    /// El primer asignado.
    pub assignee: Option<&'a str>,
    /// Todos los asignados (D20).
    pub assignees: Vec<&'a str>,
    pub epic: Option<&'a str>,
    pub milestone: Option<&'a str>,
    /// Ciclos de la tarea y si alguno es el que está en curso (RF-INT-05).
    pub cycles: Vec<&'a str>,
    pub cycle_current: bool,
    pub project: &'a str,
    pub parent: Option<&'a str>,
    pub due: Option<Date>,
    pub start: Option<Date>,
    pub created: Date,
    pub updated: Date,
    pub estimate: Option<f64>,
    pub ready: bool,
    pub claimed: bool,
    pub needs_human: bool,
    pub locked: bool,
    pub archived: bool,
    pub today: bool,
    pub stale: bool,
    pub fields: &'a BTreeMap<String, String>,
}

/// Nombres para traducir ids a lo que escribe la gente; el engine y la app
/// lo llenan desde sus datos y así arman los mismos hechos.
#[derive(Debug, Clone, Default)]
pub struct Names {
    pub projects: HashMap<ProjectId, String>,
    pub labels: HashMap<LabelId, String>,
    pub agents: HashMap<AgentId, String>,
    pub epics: HashMap<EpicId, String>,
    /// Ciclos: nombre y si está en curso (empezó y no se cerró).
    pub cycles: HashMap<model::CycleId, (String, bool)>,
}

/// Lo que no está en la tarea: su clave, la del padre y si está lista.
#[derive(Debug, Clone, Copy)]
pub struct Extra<'a> {
    pub key: &'a str,
    pub parent: Option<&'a str>,
    pub ready: bool,
    /// Bloqueo humano propio o heredado de un ancestro o épica.
    pub locked: bool,
    /// Marcada «para hoy» (RF-VIS-08).
    pub today: bool,
    /// Pendiente sin cambios desde hace semanas (RF-VIS-16).
    pub stale: bool,
}

fn local_date(ts: Timestamp) -> Date {
    ts.to_zoned(TimeZone::system()).date()
}

impl Names {
    /// Nombre con el que se escribe un actor en un filtro.
    fn actor<'a>(&'a self, a: &'a Actor) -> &'a str {
        match a {
            Actor::Agent(id) => self.agents.get(id).map_or("?", String::as_str),
            Actor::Supervisor => "supervisor",
            Actor::Interface(_) => "interface",
            Actor::Integration(n) => n.as_str(),
            Actor::System => "system",
        }
    }

    pub fn facts<'a>(&'a self, t: &'a Task, extra: Extra<'a>) -> Facts<'a> {
        let name = |map: &'a HashMap<EpicId, String>, id: Option<EpicId>| {
            id.and_then(|id| map.get(&id)).map(String::as_str)
        };
        Facts {
            key: extra.key,
            title: &t.title,
            description: &t.description,
            status: t.status,
            stage: t.stage.as_deref(),
            priority: t.priority,
            labels: t
                .labels
                .iter()
                .filter_map(|l| self.labels.get(l).map(String::as_str))
                .collect(),
            assignee: t.assignee().map(|a| self.actor(a)),
            assignees: t.assignees.iter().map(|a| self.actor(a)).collect(),
            epic: name(&self.epics, t.epic_id),
            milestone: name(&self.epics, t.milestone_id),
            cycles: t
                .cycles
                .iter()
                .filter_map(|c| self.cycles.get(c))
                .map(|(n, _)| n.as_str())
                .collect(),
            cycle_current: t
                .cycles
                .iter()
                .filter_map(|c| self.cycles.get(c))
                .any(|(_, current)| *current),
            project: self.projects.get(&t.project_id).map_or("?", String::as_str),
            parent: extra.parent,
            due: t.due.as_ref().map(model::When::date),
            start: t.start.as_ref().map(model::When::date),
            created: local_date(t.created_at),
            updated: local_date(t.updated_at),
            estimate: t.estimate.map(model::Estimate::as_f64),
            ready: extra.ready,
            claimed: t.claim.is_some(),
            needs_human: t.needs_human.is_some(),
            locked: extra.locked || t.locked,
            archived: t.archived_at.is_some(),
            today: extra.today,
            stale: extra.stale,
            fields: &t.fields,
        }
    }
}

/// Hoy en la zona del sistema, para las fechas relativas.
pub fn today() -> Date {
    local_date(Timestamp::now())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Contains,
    /// `:`: igual, o «contiene» en listas.
    Has,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    And(Vec<Expr>),
    Or(Vec<Expr>),
    Not(Box<Expr>),
    Cond {
        field: String,
        op: Op,
        value: String,
    },
    Text(String),
}

/// Error de sintaxis: posición (en caracteres) y qué se esperaba.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterError {
    pub position: usize,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Open,
    Close,
    And,
    Or,
    Not,
    Word(String),
}

fn tokenize(src: &str) -> Result<Vec<(usize, Token)>, FilterError> {
    let mut out = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars.get(i).copied().unwrap_or(' ');
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '(' || c == ')' {
            out.push((i, if c == '(' { Token::Open } else { Token::Close }));
            i += 1;
            continue;
        }
        let start = i;
        let mut word = String::new();
        while let Some(&c) = chars.get(i) {
            if c.is_whitespace() || c == '(' || c == ')' {
                break;
            }
            if c == '"' {
                i += 1;
                let mut closed = false;
                while let Some(&q) = chars.get(i) {
                    i += 1;
                    if q == '"' {
                        closed = true;
                        break;
                    }
                    word.push(q);
                }
                if !closed {
                    return Err(FilterError {
                        position: start,
                        detail: "closing quote".into(),
                    });
                }
                continue;
            }
            word.push(c);
            i += 1;
        }
        let token = match word.to_uppercase().as_str() {
            "AND" | "Y" => Token::And,
            "OR" | "O" => Token::Or,
            "NOT" | "NO" => Token::Not,
            _ => Token::Word(word),
        };
        out.push((start, token));
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<(usize, Token)>,
    at: usize,
    len: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|(_, t)| t)
    }

    fn pos(&self) -> usize {
        self.tokens.get(self.at).map_or(self.len, |(p, _)| *p)
    }

    fn or(&mut self) -> Result<Expr, FilterError> {
        let mut parts = vec![self.and()?];
        while self.peek() == Some(&Token::Or) {
            self.at += 1;
            parts.push(self.and()?);
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Expr::Or(parts)
        })
    }

    fn and(&mut self) -> Result<Expr, FilterError> {
        let mut parts = vec![self.not()?];
        loop {
            match self.peek() {
                Some(Token::And) => {
                    self.at += 1;
                    parts.push(self.not()?);
                }
                Some(Token::Not | Token::Open | Token::Word(_)) => parts.push(self.not()?),
                _ => break,
            }
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Expr::And(parts)
        })
    }

    fn not(&mut self) -> Result<Expr, FilterError> {
        let pos = self.pos();
        match self.peek().cloned() {
            Some(Token::Not) => {
                self.at += 1;
                Ok(Expr::Not(Box::new(self.not()?)))
            }
            Some(Token::Open) => {
                self.at += 1;
                let inner = self.or()?;
                if self.peek() != Some(&Token::Close) {
                    return Err(FilterError {
                        position: self.pos(),
                        detail: "`)`".into(),
                    });
                }
                self.at += 1;
                Ok(inner)
            }
            Some(Token::Word(w)) => {
                self.at += 1;
                if let Some(rest) = w.strip_prefix('-').filter(|r| !r.is_empty()) {
                    return Ok(Expr::Not(Box::new(condition(rest))));
                }
                Ok(condition(&w))
            }
            _ => Err(FilterError {
                position: pos,
                detail: "a condition".into(),
            }),
        }
    }
}

/// `campo<op>valor` si empieza por un nombre de campo; si no, texto suelto.
fn condition(word: &str) -> Expr {
    let name_end = word
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .unwrap_or(word.len());
    let (field, rest) = word.split_at(name_end);
    for (sym, op) in [
        ("<=", Op::Le),
        (">=", Op::Ge),
        ("!=", Op::Ne),
        ("=", Op::Eq),
        ("<", Op::Lt),
        (">", Op::Gt),
        ("~", Op::Contains),
        (":", Op::Has),
    ] {
        if !field.is_empty()
            && let Some(value) = rest.strip_prefix(sym)
        {
            return Expr::Cond {
                field: field.to_lowercase(),
                op,
                value: value.to_owned(),
            };
        }
    }
    Expr::Text(word.to_owned())
}

/// Lee un filtro; vacío = todo.
pub fn parse(src: &str) -> Result<Expr, FilterError> {
    let tokens = tokenize(src)?;
    if tokens.is_empty() {
        return Ok(Expr::And(Vec::new()));
    }
    let len = src.chars().count();
    let mut p = Parser { tokens, at: 0, len };
    let expr = p.or()?;
    if p.at < p.tokens.len() {
        return Err(FilterError {
            position: p.pos(),
            detail: "an operator or the end".into(),
        });
    }
    Ok(expr)
}

/// `today`, `today+3d`, `today-2w`, `today+1m` o `AAAA-MM-DD`.
pub fn date_value(value: &str, today: Date) -> Option<Date> {
    let v = value.trim().to_lowercase();
    let rel = v.strip_prefix("today").or_else(|| v.strip_prefix("hoy"));
    let Some(rel) = rel else {
        return v.parse().ok();
    };
    if rel.is_empty() {
        return Some(today);
    }
    let (sign, num) = match rel.split_at(1) {
        ("+", n) => (1i64, n),
        ("-", n) => (-1i64, n),
        _ => return None,
    };
    let unit = num.chars().last()?;
    let amount: i64 = num.get(..num.len().saturating_sub(1))?.parse().ok()?;
    let span = match unit {
        'd' => jiff::Span::new().days(sign * amount),
        'w' => jiff::Span::new().weeks(sign * amount),
        'm' => jiff::Span::new().months(sign * amount),
        'y' => jiff::Span::new().years(sign * amount),
        _ => return None,
    };
    today.checked_add(span).ok()
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn text_cmp(actual: Option<&str>, op: Op, value: &str) -> bool {
    let none = value.eq_ignore_ascii_case("none");
    match (actual, op) {
        (None, Op::Eq | Op::Has) => none,
        (None, Op::Ne) => !none,
        (None, _) => false,
        (Some(a), Op::Eq | Op::Has) => eq_ci(a, value),
        (Some(a), Op::Ne) => !eq_ci(a, value),
        (Some(a), Op::Contains) => a.to_lowercase().contains(&value.to_lowercase()),
        (Some(a), op) => ordered(a.to_lowercase().cmp(&value.to_lowercase()), op),
    }
}

/// Un nombre dentro de una lista: `:`/`=` alguno es; `!=` ninguno es;
/// `none` = lista vacía.
fn list_cmp(list: &[&str], op: Op, value: &str) -> bool {
    if value.eq_ignore_ascii_case("none") {
        return match op {
            Op::Ne => !list.is_empty(),
            _ => list.is_empty(),
        };
    }
    match op {
        Op::Ne => !list.iter().any(|a| eq_ci(a, value)),
        Op::Contains => list
            .iter()
            .any(|a| a.to_lowercase().contains(&value.to_lowercase())),
        _ => list.iter().any(|a| eq_ci(a, value)),
    }
}

fn ordered(ord: std::cmp::Ordering, op: Op) -> bool {
    use std::cmp::Ordering::*;
    match op {
        Op::Lt => ord == Less,
        Op::Le => ord != Greater,
        Op::Gt => ord == Greater,
        Op::Ge => ord != Less,
        Op::Eq | Op::Has => ord == Equal,
        Op::Ne => ord != Equal,
        Op::Contains => false,
    }
}

fn date_cmp(actual: Option<Date>, op: Op, value: &str, today: Date) -> bool {
    let none = value.eq_ignore_ascii_case("none");
    match (actual, date_value(value, today)) {
        (None, _) => none && matches!(op, Op::Eq | Op::Has) || !none && op == Op::Ne,
        (Some(_), _) if none => op == Op::Ne,
        (Some(a), Some(v)) => ordered(a.cmp(&v), op),
        (Some(_), None) => false,
    }
}

fn num_cmp(actual: Option<f64>, op: Op, value: &str) -> bool {
    match (actual, value.replace(',', ".").parse::<f64>()) {
        (Some(a), Ok(v)) => a.partial_cmp(&v).is_some_and(|o| ordered(o, op)),
        (None, _) => value.eq_ignore_ascii_case("none") && matches!(op, Op::Eq | Op::Has),
        _ => false,
    }
}

fn bool_cmp(actual: bool, op: Op, value: &str) -> bool {
    let wanted = !matches!(value.to_lowercase().as_str(), "false" | "no" | "0");
    match op {
        Op::Ne => actual != wanted,
        _ => actual == wanted,
    }
}

fn eval_cond(f: &Facts<'_>, field: &str, op: Op, value: &str, today: Date) -> bool {
    match field {
        "status" | "estado" => match value.parse::<Status>() {
            Ok(s) => match op {
                Op::Ne => f.status != s,
                _ => f.status == s,
            },
            // Una etapa también vale como estado.
            Err(_) => text_cmp(f.stage, op, value),
        },
        "stage" | "etapa" => text_cmp(f.stage, op, value),
        "priority" | "prioridad" => match value.parse::<Priority>() {
            // Más prioritaria = mayor: se compara al revés del rango.
            Ok(p) => ordered(p.rank().cmp(&f.priority.rank()), op),
            Err(_) => false,
        },
        "label" | "etiqueta" => match op {
            Op::Ne => !f.labels.iter().any(|l| eq_ci(l, value)),
            Op::Contains => f
                .labels
                .iter()
                .any(|l| l.to_lowercase().contains(&value.to_lowercase())),
            _ => f.labels.iter().any(|l| eq_ci(l, value)),
        },
        // Cualquiera de los asignados (D20).
        "assignee" | "asignado" => list_cmp(&f.assignees, op, value),
        "epic" | "epica" | "épica" => text_cmp(f.epic, op, value),
        "cycle" | "ciclo" if matches!(value, "current" | "actual") => {
            bool_cmp(f.cycle_current, op, "true")
        }
        // Cualquiera de sus ciclos.
        "cycle" | "ciclo" => list_cmp(&f.cycles, op, value),
        "milestone" | "hito" => text_cmp(f.milestone, op, value),
        "project" | "proyecto" => text_cmp(Some(f.project), op, value),
        "parent" | "padre" => text_cmp(f.parent, op, value),
        "key" | "clave" => text_cmp(Some(f.key), op, value),
        "title" | "titulo" | "título" => text_cmp(Some(f.title), op, value),
        "text" | "texto" => text_matches(f, value),
        "due" | "vence" => date_cmp(f.due, op, value, today),
        "start" | "inicio" => date_cmp(f.start, op, value, today),
        "created" | "creada" => date_cmp(Some(f.created), op, value, today),
        "updated" | "cambiada" => date_cmp(Some(f.updated), op, value, today),
        "estimate" | "estimacion" | "estimación" => num_cmp(f.estimate, op, value),
        "ready" | "lista" => bool_cmp(f.ready, op, value),
        "blocked" | "bloqueada" => bool_cmp(!f.ready, op, value),
        "claimed" | "tomada" => bool_cmp(f.claimed, op, value),
        "needs_human" | "pregunta" => bool_cmp(f.needs_human, op, value),
        "locked" | "bloqueo" => bool_cmp(f.locked, op, value),
        "archived" | "archivada" => bool_cmp(f.archived, op, value),
        "closed" | "cerrada" => bool_cmp(f.status.category().is_closed(), op, value),
        "today" | "hoy" => bool_cmp(f.today, op, value),
        "stale" | "envejecida" => bool_cmp(f.stale, op, value),
        custom => {
            let key = custom.strip_prefix("field.").unwrap_or(custom);
            let actual = f.fields.get(key).map(String::as_str);
            if op == Op::Has
                && let Some(a) = actual
                && a.contains(',')
            {
                return a.split(',').any(|x| eq_ci(x, value));
            }
            match (
                actual.and_then(|a| a.parse::<f64>().ok()),
                value.parse::<f64>(),
            ) {
                (Some(a), Ok(_)) if !matches!(op, Op::Contains) => num_cmp(Some(a), op, value),
                _ => text_cmp(actual, op, value),
            }
        }
    }
}

fn text_matches(f: &Facts<'_>, text: &str) -> bool {
    let t = text.to_lowercase();
    f.key.to_lowercase().contains(&t)
        || f.title.to_lowercase().contains(&t)
        || f.description.to_lowercase().contains(&t)
}

/// ¿Cumple la tarea el filtro?
pub fn matches(expr: &Expr, f: &Facts<'_>, today: Date) -> bool {
    match expr {
        Expr::And(parts) => parts.iter().all(|e| matches(e, f, today)),
        Expr::Or(parts) => parts.iter().any(|e| matches(e, f, today)),
        Expr::Not(inner) => !matches(inner, f, today),
        Expr::Cond { field, op, value } => eval_cond(f, field, *op, value, today),
        Expr::Text(t) => text_matches(f, t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts<'a>(fields: &'a BTreeMap<String, String>) -> Facts<'a> {
        let d = |s: &str| s.parse::<Date>().unwrap();
        Facts {
            key: "API-7",
            title: "Migrar el esquema",
            description: "con índices",
            status: Status::InProgress,
            stage: None,
            priority: Priority::High,
            labels: vec!["db", "rust"],
            assignee: Some("claude-1"),
            assignees: vec!["claude-1", "revisor-1"],
            epic: Some("Pagos"),
            milestone: None,
            cycles: vec!["Sprint 1", "Sprint 2"],
            cycle_current: true,
            project: "API",
            parent: None,
            due: Some(d("2026-11-02")),
            start: None,
            created: d("2026-10-01"),
            updated: d("2026-10-20"),
            estimate: Some(3.0),
            ready: true,
            claimed: true,
            needs_human: false,
            locked: false,
            archived: false,
            today: false,
            stale: false,
            fields,
        }
    }

    fn check(src: &str) -> bool {
        let mut fields = BTreeMap::new();
        fields.insert("cliente".to_owned(), "Acme".to_owned());
        fields.insert("plataformas".to_owned(), "Linux,Mac".to_owned());
        let today: Date = "2026-10-31".parse().unwrap();
        matches(&parse(src).unwrap(), &facts(&fields), today)
    }

    #[test]
    fn conditions_combine_with_and_or_not() {
        assert!(check("status:in_progress priority>=high"));
        assert!(!check("priority>high"), "urgent > high");
        assert!(check("label:db AND -label:docs"));
        assert!(check("(label:docs OR label:rust) assignee:claude-1"));
        assert!(check("assignee:revisor-1 -assignee:nadie"));
        assert!(check("NOT status:done"));
        assert!(!check("status:done OR blocked:true"));
        assert!(check("epic:pagos milestone:none"));
        assert!(check("closed:false -today:true"));
        assert!(check("title~esquema"));
        assert!(check("migrar"), "texto suelto");
        assert!(check(r#"title~"el esquema""#));
        assert!(check("cliente=acme plataformas:mac estimate>2"));
        assert!(check(""));
    }

    #[test]
    fn relative_dates() {
        assert!(check("due<=today+3d"));
        assert!(!check("due<today"));
        assert!(check("created>=today-1m updated<today"));
        assert!(check("start:none"));
        let today: Date = "2026-01-31".parse().unwrap();
        assert_eq!(
            date_value("today+1w", today),
            Some("2026-02-07".parse().unwrap())
        );
        assert_eq!(
            date_value("2026-05-01", today),
            Some("2026-05-01".parse().unwrap())
        );
        assert_eq!(date_value("ayer", today), None);
    }

    #[test]
    fn syntax_errors_say_where() {
        let e = parse("(status:todo").unwrap_err();
        assert_eq!(e.detail, "`)`");
        let e = parse(r#"title~"abierto"#).unwrap_err();
        assert_eq!(e.position, 0);
        assert!(parse("status:todo OR").is_err());
    }
}
