use crate::error::{Error, Result};
use crate::task::TaskId;

pub fn check_no_cycle(from: &TaskId, reachable_from_to: &[TaskId]) -> Result<()> {
    if reachable_from_to.contains(from) {
        return Err(Error::InvalidInput(
            "dependency would create a cycle".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_cycle_when_reachable_set_empty() {
        let from = TaskId::new();
        assert!(check_no_cycle(&from, &[]).is_ok());
    }

    #[test]
    fn detects_cycle_when_from_in_reachable_set() {
        let from = TaskId::new();
        let reachable = vec![from];
        assert!(check_no_cycle(&from, &reachable).is_err());
    }

    #[test]
    fn no_cycle_when_from_not_in_reachable_set() {
        let from = TaskId::new();
        let other = TaskId::new();
        assert!(check_no_cycle(&from, &[other]).is_ok());
    }
}
