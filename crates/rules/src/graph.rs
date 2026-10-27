//! Grafo de dependencias de un proyecto (RF-REL-05) y su ruta crítica
//! (RF-REL-06). El grafo no tiene ciclos: `find_cycle` los impide al crear
//! cada dependencia.

use std::collections::{BTreeMap, HashMap, HashSet};

use model::{Dependency, Task, TaskId};

/// De dónde sale el peso de una tarea en la ruta crítica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    /// Su estimación.
    Estimate,
    /// Días de su inicio a su vencimiento.
    Days,
    /// Sin datos: cuenta 1.
    One,
}

impl Weight {
    pub fn key(self) -> &'static str {
        match self {
            Self::Estimate => "estimate",
            Self::Days => "days",
            Self::One => "one",
        }
    }
}

/// Peso de una tarea: su estimación, o los días de inicio a vencimiento, o 1.
pub fn weight(t: &Task) -> (f64, Weight) {
    if let Some(e) = t.estimate.filter(|e| e.0 > 0) {
        return (e.as_f64(), Weight::Estimate);
    }
    if let (Some(start), Some(due)) = (&t.start, &t.due) {
        let days = (due.date() - start.date()).get_days();
        if days >= 0 {
            return (f64::from(days + 1), Weight::Days);
        }
    }
    (1.0, Weight::One)
}

/// Aristas `blocker → blocked` entre las tareas dadas.
fn edges(ids: &HashSet<TaskId>, deps: &[Dependency]) -> Vec<(TaskId, TaskId)> {
    deps.iter()
        .filter(|d| ids.contains(&d.blocker) && ids.contains(&d.blocked))
        .map(|d| (d.blocker, d.blocked))
        .collect()
}

/// Orden topológico (Kahn), estable según el orden de `tasks`.
fn topo(tasks: &[Task], edges: &[(TaskId, TaskId)]) -> Vec<TaskId> {
    let mut indegree: HashMap<TaskId, usize> = tasks.iter().map(|t| (t.id, 0)).collect();
    let mut out_edges: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
    for (a, b) in edges {
        *indegree.entry(*b).or_default() += 1;
        out_edges.entry(*a).or_default().push(*b);
    }
    let position: HashMap<TaskId, usize> =
        tasks.iter().enumerate().map(|(i, t)| (t.id, i)).collect();
    // Listas por posición para que el resultado no dependa del hash.
    let mut ready: BTreeMap<usize, TaskId> = tasks
        .iter()
        .filter(|t| indegree.get(&t.id) == Some(&0))
        .filter_map(|t| position.get(&t.id).map(|p| (*p, t.id)))
        .collect();
    let mut order = Vec::with_capacity(tasks.len());
    while let Some((_, id)) = ready.pop_first() {
        order.push(id);
        for next in out_edges.get(&id).into_iter().flatten() {
            if let Some(n) = indegree.get_mut(next) {
                *n = n.saturating_sub(1);
                if *n == 0
                    && let Some(p) = position.get(next)
                {
                    ready.insert(*p, *next);
                }
            }
        }
    }
    order
}

/// Capa de cada tarea: 0 para las que no dependen de nada; si no, una más que
/// la más profunda de sus bloqueadoras.
pub fn layers(tasks: &[Task], deps: &[Dependency]) -> HashMap<TaskId, usize> {
    let ids: HashSet<TaskId> = tasks.iter().map(|t| t.id).collect();
    let edges = edges(&ids, deps);
    let mut layer: HashMap<TaskId, usize> = HashMap::new();
    for id in topo(tasks, &edges) {
        let depth = edges
            .iter()
            .filter(|(_, b)| *b == id)
            .filter_map(|(a, _)| layer.get(a).map(|l| l + 1))
            .max()
            .unwrap_or(0);
        layer.insert(id, depth);
    }
    layer
}

/// La cadena de dependencias más pesada entre las tareas dadas (normalmente,
/// las abiertas): es la que fija cuándo puede terminar el proyecto. Devuelve
/// la cadena, de la primera a la última, y su peso total.
pub fn critical_path(tasks: &[Task], deps: &[Dependency]) -> (Vec<TaskId>, f64) {
    let ids: HashSet<TaskId> = tasks.iter().map(|t| t.id).collect();
    let edges = edges(&ids, deps);
    let weights: HashMap<TaskId, f64> = tasks.iter().map(|t| (t.id, weight(t).0)).collect();
    // Mejor peso acumulado que termina en cada tarea y de dónde viene.
    let mut best: HashMap<TaskId, (f64, Option<TaskId>)> = HashMap::new();
    for id in topo(tasks, &edges) {
        let own = weights.get(&id).copied().unwrap_or(1.0);
        let from = edges
            .iter()
            .filter(|(_, b)| *b == id)
            .filter_map(|(a, _)| best.get(a).map(|(w, _)| (*w, *a)))
            .fold(None, |acc: Option<(f64, TaskId)>, (w, a)| match acc {
                Some((bw, _)) if bw >= w => acc,
                _ => Some((w, a)),
            });
        let total = own + from.map_or(0.0, |(w, _)| w);
        best.insert(id, (total, from.map(|(_, a)| a)));
    }
    // El final de la cadena: el mayor total (a igualdad, el primero en la lista).
    let Some(end) = tasks
        .iter()
        .filter_map(|t| best.get(&t.id).map(|(w, _)| (t.id, *w)))
        .fold(None, |acc: Option<(TaskId, f64)>, (id, w)| match acc {
            Some((_, bw)) if bw >= w => acc,
            _ => Some((id, w)),
        })
    else {
        return (Vec::new(), 0.0);
    };
    let mut path = vec![end.0];
    let mut at = end.0;
    while let Some((_, Some(prev))) = best.get(&at) {
        path.push(*prev);
        at = *prev;
    }
    path.reverse();
    (path, end.1)
}

/// El grafo en formato DOT de Graphviz, con la ruta crítica en rojo.
pub fn dot(
    name: &str,
    nodes: &[(String, String, bool)],
    edges: &[(String, String, bool)],
) -> String {
    let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut out = format!(
        "digraph \"{}\" {{\n  rankdir=LR;\n  node [shape=box];\n",
        quote(name)
    );
    for (key, title, critical) in nodes {
        let color = if *critical { ", color=red" } else { "" };
        out.push_str(&format!(
            "  \"{}\" [label=\"{}\\n{}\"{color}];\n",
            quote(key),
            quote(key),
            quote(title)
        ));
    }
    for (a, b, critical) in edges {
        let color = if *critical { " [color=red]" } else { "" };
        out.push_str(&format!("  \"{}\" -> \"{}\"{color};\n", quote(a), quote(b)));
    }
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use model::{Actor, Estimate, Priority, ProjectId, Status, Timestamp};

    use super::*;

    fn task(n: u64, estimate: Option<u32>) -> Task {
        Task {
            id: TaskId::generate(),
            project_id: ProjectId::generate(),
            number: n,
            parent_id: None,
            title: format!("t{n}"),
            description: String::new(),
            status: Status::Todo,
            priority: Priority::Medium,
            start: None,
            due: None,
            labels: vec![],
            assignees: vec![],
            creator: Actor::Supervisor,
            checklist: vec![],
            criteria: vec![],
            revision: 1,
            created_at: Timestamp::UNIX_EPOCH,
            updated_at: Timestamp::UNIX_EPOCH,
            archived_at: None,
            deleted_at: None,
            claim: None,
            needs_human: None,
            locked: false,
            stage: None,
            estimate: estimate.map(|e| Estimate(e * 100)),
            epic_id: None,
            milestone_id: None,
            fields: Default::default(),
            recurrence: None,
            cycles: Vec::new(),
        }
    }

    fn dep(a: &Task, b: &Task) -> Dependency {
        Dependency {
            blocker: a.id,
            blocked: b.id,
        }
    }

    #[test]
    fn layers_and_the_heaviest_chain() {
        //   1(2) → 2(1) → 4(1)
        //   1(2) → 3(5) → 4(1)
        //   5(1)   (suelta)
        let t: Vec<Task> = [2, 1, 5, 1, 1]
            .iter()
            .zip(1..)
            .map(|(e, n)| task(n, Some(*e)))
            .collect();
        let deps = vec![
            dep(&t[0], &t[1]),
            dep(&t[0], &t[2]),
            dep(&t[1], &t[3]),
            dep(&t[2], &t[3]),
        ];
        let l = layers(&t, &deps);
        assert_eq!(
            t.iter().map(|x| l[&x.id]).collect::<Vec<_>>(),
            [0, 1, 1, 2, 0]
        );
        let (path, total) = critical_path(&t, &deps);
        assert_eq!(path, [t[0].id, t[2].id, t[3].id]);
        assert!((total - 8.0).abs() < 1e-9);
    }

    #[test]
    fn without_estimates_each_task_weighs_one() {
        let t: Vec<Task> = (1..=3).map(|n| task(n, None)).collect();
        let deps = vec![dep(&t[0], &t[1])];
        let (path, total) = critical_path(&t, &deps);
        assert_eq!(path, [t[0].id, t[1].id]);
        assert!((total - 2.0).abs() < 1e-9);
        assert_eq!(critical_path(&[], &[]), (vec![], 0.0));
        let g = dot("API", &[("API-1".into(), "a \"b\"".into(), true)], &[]);
        assert!(g.contains("\\\"b\\\""), "{g}");
    }
}
