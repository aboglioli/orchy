use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::error::{DomainError, Result};
use crate::id::Id;

#[derive(Debug, Clone, Default)]
pub struct Waits(BTreeMap<Id, BTreeSet<Id>>);

impl Waits {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, waiter: Id, on: Id) {
        self.0.entry(waiter).or_default().insert(on);
    }

    pub fn remove(&mut self, waiter: &Id, on: &Id) {
        if let Some(targets) = self.0.get_mut(waiter) {
            targets.remove(on);
        }
    }

    pub fn on(&self, waiter: &Id) -> impl Iterator<Item = &Id> {
        self.0.get(waiter).into_iter().flatten()
    }

    pub fn ensure_no_loop(&self, adding: &[(Id, Id)]) -> Result<()> {
        let mut after = self.clone();
        for (waiter, on) in adding {
            if waiter == on {
                return Err(loop_through(&[waiter.clone(), on.clone()]));
            }
            after.add(waiter.clone(), on.clone());
        }
        for (waiter, on) in adding {
            if let Some(path) = after.path(on, waiter) {
                let mut cycle = vec![waiter.clone()];
                cycle.extend(path);
                return Err(loop_through(&cycle));
            }
        }
        Ok(())
    }

    pub fn reachable(&self, from: &Id) -> BTreeSet<Id> {
        let mut seen = BTreeSet::from([from.clone()]);
        let mut queue = VecDeque::from([from.clone()]);
        while let Some(node) = queue.pop_front() {
            for next in self.on(&node) {
                if seen.insert(next.clone()) {
                    queue.push_back(next.clone());
                }
            }
        }
        seen
    }

    pub fn loops(&self) -> Vec<Vec<Id>> {
        let mut found: Vec<Vec<Id>> = Vec::new();
        let mut seen_sets: BTreeSet<Vec<Id>> = BTreeSet::new();
        for start in self.0.keys() {
            for next in self.on(start) {
                let Some(mut path) = self.path(next, start) else {
                    continue;
                };
                path.pop();
                let mut cycle = vec![start.clone()];
                cycle.extend(path);
                let mut members = cycle.clone();
                members.sort();
                members.dedup();
                if !seen_sets.insert(members) {
                    continue;
                }
                let smallest = cycle
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.cmp(b.1))
                    .map_or(0, |(i, _)| i);
                cycle.rotate_left(smallest);
                found.push(cycle);
            }
        }
        found
    }

    fn path(&self, from: &Id, to: &Id) -> Option<Vec<Id>> {
        let mut came_from: BTreeMap<Id, Id> = BTreeMap::new();
        let mut queue = VecDeque::from([from.clone()]);
        let mut seen = BTreeSet::from([from.clone()]);
        while let Some(node) = queue.pop_front() {
            if &node == to {
                let mut path = vec![node.clone()];
                let mut cursor = node;
                while let Some(previous) = came_from.get(&cursor) {
                    path.push(previous.clone());
                    cursor = previous.clone();
                }
                path.reverse();
                return Some(path);
            }
            for next in self.on(&node) {
                if seen.insert(next.clone()) {
                    came_from.insert(next.clone(), node.clone());
                    queue.push_back(next.clone());
                }
            }
        }
        None
    }
}

fn loop_through(cycle: &[Id]) -> DomainError {
    let walk: Vec<String> = cycle.iter().map(ToString::to_string).collect();
    DomainError::validation(format!(
        "that would make work wait on itself ({}), so none of it could ever finish",
        walk.join(" waits on ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> Id {
        Id::new(format!("01ARZ3NDEKTSV4RRFFQ69G5FA{}", (b'A' + n) as char)).unwrap()
    }

    #[test]
    fn a_dependency_on_something_already_waiting_on_you_is_a_loop() {
        let mut waits = Waits::new();
        waits.add(id(1), id(2));
        let err = waits.ensure_no_loop(&[(id(2), id(1))]).unwrap_err();
        assert!(err.to_string().contains("wait on itself"), "{err}");
    }

    #[test]
    fn a_loop_through_a_subtask_and_a_dependency_is_still_a_loop() {
        let mut waits = Waits::new();
        waits.add(id(1), id(2));
        assert!(
            waits.ensure_no_loop(&[(id(2), id(1))]).is_err(),
            "a subtask waiting on its own goal can never start, and the goal never finish"
        );
    }

    #[test]
    fn independent_work_and_long_chains_are_fine() {
        let mut waits = Waits::new();
        waits.add(id(1), id(2));
        waits.add(id(2), id(3));
        assert!(waits.ensure_no_loop(&[(id(4), id(1))]).is_ok());
        assert!(waits.ensure_no_loop(&[(id(1), id(3))]).is_ok());
    }

    #[test]
    fn nothing_waits_on_itself() {
        assert!(Waits::new().ensure_no_loop(&[(id(1), id(1))]).is_err());
    }

    #[test]
    fn a_removed_wait_no_longer_closes_a_loop() {
        let mut waits = Waits::new();
        waits.add(id(1), id(2));
        waits.remove(&id(1), &id(2));
        assert!(waits.ensure_no_loop(&[(id(2), id(1))]).is_ok());
    }

    #[test]
    fn existing_loops_are_found_once_each() {
        let mut waits = Waits::new();
        waits.add(id(1), id(2));
        waits.add(id(2), id(3));
        waits.add(id(3), id(1));
        waits.add(id(4), id(1));
        let loops = waits.loops();
        assert_eq!(loops, vec![vec![id(1), id(2), id(3)]]);
    }

    #[test]
    fn the_error_walks_the_loop() {
        let mut waits = Waits::new();
        waits.add(id(1), id(2));
        waits.add(id(2), id(3));
        let err = waits
            .ensure_no_loop(&[(id(3), id(1))])
            .unwrap_err()
            .to_string();
        for n in 1..=3 {
            assert!(err.contains(&id(n).to_string()), "{err}");
        }
    }
}
