use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::actor::{Actor, ActorAlias, ActorId, Role};
use crate::error::{DomainError, Result};
use crate::namespace::Namespace;

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Recipient {
    Actor(ActorAlias),
    Instance(ActorId),
    Role(Role),
    Namespace(Namespace),
    Broadcast,
}

impl Recipient {
    pub fn delivers_to(&self, actor: &Actor, sender: &ActorId) -> bool {
        match self {
            Self::Actor(alias) => actor.id().alias() == alias,
            Self::Instance(id) => actor.id() == id,
            Self::Role(role) => actor.has_role(role),
            Self::Namespace(ns) => ns.contains(actor.namespace()),
            Self::Broadcast => actor.id() != sender,
        }
    }
}

impl fmt::Display for Recipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Actor(alias) => write!(f, "@{alias}"),
            Self::Instance(id) => write!(f, "@{id}"),
            Self::Role(role) => write!(f, "role:{role}"),
            Self::Namespace(ns) => write!(f, "ns:{ns}"),
            Self::Broadcast => f.write_str("broadcast"),
        }
    }
}

impl FromStr for Recipient {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        let s = s.trim();
        if s == "broadcast" {
            return Ok(Self::Broadcast);
        }
        if let Some(role) = s.strip_prefix("role:") {
            return Ok(Self::Role(Role::new(role)?));
        }
        if let Some(ns) = s.strip_prefix("ns:") {
            return Ok(Self::Namespace(Namespace::new(ns)?));
        }
        let handle = s.strip_prefix('@').ok_or_else(|| {
            DomainError::validation(format!(
                "`{s}` is not a recipient (expected @alias, @alias@machine, role:, ns: or broadcast)"
            ))
        })?;
        if handle.contains('@') {
            return Ok(Self::Instance(handle.parse()?));
        }
        Ok(Self::Actor(ActorAlias::new(handle)?))
    }
}

impl TryFrom<String> for Recipient {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        value.parse()
    }
}

impl From<Recipient> for String {
    fn from(recipient: Recipient) -> Self {
        recipient.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Clock;
    use chrono::{DateTime, Utc};

    const M1: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const M2: &str = "01BX5ZZKBKACTAV9WEVGEMMVRZ";

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            DateTime::from_timestamp(1_700_000_000, 0).unwrap()
        }
    }

    fn actor(alias: &str, machine: &str, role: &str, ns: &str) -> Actor {
        Actor::announce(
            ActorId::new(alias, machine).unwrap(),
            vec![Role::new(role).unwrap()],
            Namespace::new(ns).unwrap(),
            &FixedClock,
        )
    }

    fn parse(s: &str) -> Recipient {
        s.parse().unwrap()
    }

    #[test]
    fn every_addressing_mode_round_trips() {
        for s in [
            "@claude",
            &format!("@claude@{M1}"),
            "role:reviewer",
            "ns:/backend",
            "broadcast",
        ] {
            assert_eq!(parse(s).to_string(), s, "round trip for {s}");
        }
    }

    #[test]
    fn an_alias_reaches_every_instance_but_an_instance_reaches_one() {
        let here = actor("claude", M1, "dev", "/");
        let there = actor("claude", M2, "dev", "/");
        let sender = ActorId::new("codex", M1).unwrap();

        assert!(parse("@claude").delivers_to(&here, &sender));
        assert!(parse("@claude").delivers_to(&there, &sender));

        let one = parse(&format!("@claude@{M1}"));
        assert!(one.delivers_to(&here, &sender));
        assert!(
            !one.delivers_to(&there, &sender),
            "an instance address is exact"
        );
    }

    #[test]
    fn a_role_reaches_whoever_holds_it() {
        let reviewer = actor("claude", M1, "reviewer", "/");
        let developer = actor("codex", M1, "developer", "/");
        let sender = ActorId::new("alan", M1).unwrap();

        assert!(parse("role:reviewer").delivers_to(&reviewer, &sender));
        assert!(!parse("role:reviewer").delivers_to(&developer, &sender));
    }

    #[test]
    fn a_namespace_address_covers_the_subtree() {
        let deep = actor("claude", M1, "dev", "/backend/auth");
        let other = actor("codex", M1, "dev", "/frontend");
        let sender = ActorId::new("alan", M1).unwrap();

        assert!(parse("ns:/backend").delivers_to(&deep, &sender));
        assert!(!parse("ns:/backend").delivers_to(&other, &sender));
    }

    #[test]
    fn broadcast_reaches_everyone_except_the_sender() {
        let sender_id = ActorId::new("claude", M1).unwrap();
        let sender = actor("claude", M1, "dev", "/");
        let other = actor("codex", M1, "dev", "/");

        assert!(!parse("broadcast").delivers_to(&sender, &sender_id));
        assert!(parse("broadcast").delivers_to(&other, &sender_id));
    }

    #[test]
    fn rejects_an_unaddressed_string() {
        assert!("claude".parse::<Recipient>().is_err());
        assert!("everyone".parse::<Recipient>().is_err());
        assert!("role:".parse::<Recipient>().is_err());
        assert!("ns:backend".parse::<Recipient>().is_err());
    }
}
