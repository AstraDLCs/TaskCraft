use std::collections::{HashMap, HashSet, VecDeque};

use model::{Dependency, Status, TaskId};

/// Si añadir `new` cerraría un ciclo, devuelve el ciclo empezando y
/// terminando en `new.blocker` (RF-REL-02).
pub fn find_cycle(edges: &[Dependency], new: Dependency) -> Option<Vec<TaskId>> {
    if new.blocker == new.blocked {
        return Some(vec![new.blocker, new.blocker]);
    }
    let mut next: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
    for e in edges {
        next.entry(e.blocker).or_default().push(e.blocked);
    }

    // Camino blocked → … → blocker por las aristas existentes.
    let mut came_from: HashMap<TaskId, TaskId> = HashMap::new();
    let mut seen = HashSet::from([new.blocked]);
    let mut queue = VecDeque::from([new.blocked]);
    while let Some(current) = queue.pop_front() {
        if current == new.blocker {
            let mut path = vec![new.blocker];
            let mut at = current;
            while let Some(&prev) = came_from.get(&at) {
                path.push(prev);
                at = prev;
            }
            // path = blocker ← … ← blocked; el ciclo es blocker → blocked → … → blocker.
            path.reverse();
            path.insert(0, new.blocker);
            return Some(path);
        }
        for &n in next.get(&current).into_iter().flatten() {
            if seen.insert(n) {
                came_from.insert(n, current);
                queue.push_back(n);
            }
        }
    }
    None
}

/// Una tarea está *lista* si ninguno de sus bloqueadores sigue abierto
/// (RF-REL-01). Los bloqueadores borrados no cuentan.
/// Estados cuya categoría es terminada o cancelada: un bloqueador en uno de
/// ellos ya no bloquea.
pub fn closed_statuses() -> Vec<Status> {
    Status::ALL
        .iter()
        .copied()
        .filter(|s| s.category().is_closed())
        .collect()
}

pub fn is_ready(open_blocker_statuses: impl IntoIterator<Item = Status>) -> bool {
    open_blocker_statuses
        .into_iter()
        .all(|s| s.category().is_closed())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<TaskId> {
        (0..n).map(|_| TaskId::generate()).collect()
    }

    fn dep(blocker: TaskId, blocked: TaskId) -> Dependency {
        Dependency { blocker, blocked }
    }

    #[test]
    fn detects_indirect_cycle_with_path() {
        let t = ids(3);
        let (a, b, c) = (t[0], t[1], t[2]);
        let edges = [dep(a, b), dep(b, c)];
        assert_eq!(find_cycle(&edges, dep(c, a)), Some(vec![c, a, b, c]));
        assert_eq!(find_cycle(&edges, dep(a, c)), None);
    }

    #[test]
    fn self_dependency_is_a_cycle() {
        let a = TaskId::generate();
        assert!(find_cycle(&[], dep(a, a)).is_some());
    }

    #[test]
    fn readiness() {
        assert!(is_ready([]));
        assert!(is_ready([Status::Done, Status::Cancelled]));
        assert!(!is_ready([Status::Done, Status::InReview]));
    }
}
