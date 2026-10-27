use model::Status;

use crate::RuleError;

/// Estados a los que se puede pasar desde `from` (RF-MOD-06).
pub fn allowed_targets(from: Status) -> &'static [Status] {
    use Status::*;
    match from {
        Backlog => &[Todo, InProgress, Cancelled],
        Todo => &[Backlog, InProgress, Cancelled],
        InProgress => &[Todo, InReview, Done, Cancelled],
        InReview => &[InProgress, Todo, Done, Cancelled],
        // Reabrir.
        Done => &[Todo, InProgress],
        Cancelled => &[Backlog, Todo],
    }
}

/// Valida un cambio de estado. Cualquiera puede cerrar: la revisión ya no es
/// obligatoria (D20).
pub fn check_transition(from: Status, to: Status) -> Result<(), RuleError> {
    if from == to {
        return Ok(());
    }
    let allowed = allowed_targets(from);
    if !allowed.contains(&to) {
        return Err(RuleError::Transition {
            from,
            to,
            allowed: allowed
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_and_invalid_moves() {
        check_transition(Status::Todo, Status::InProgress).unwrap();
        check_transition(Status::Done, Status::Todo).unwrap();
        check_transition(Status::InProgress, Status::Done).unwrap();
        let err = check_transition(Status::Backlog, Status::Done).unwrap_err();
        assert!(matches!(err, RuleError::Transition { .. }));
    }
}
