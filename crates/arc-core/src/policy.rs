use arc_protocol::PermissionDecision;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionClass {
    Normal,
    Purchase,
    ExternalSend,
    CredentialChange,
    IrreversibleDeletion,
    PrivilegedSystem,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyEvaluation {
    pub decision: PermissionDecision,
    pub reason: String,
}

pub fn evaluate(class: ActionClass, explicitly_confirmed: bool) -> PolicyEvaluation {
    let reason = match class {
        ActionClass::Normal => "Action is within the active user task",
        ActionClass::Purchase => "Purchases require explicit confirmation",
        ActionClass::ExternalSend => "Sending or publishing as the user requires confirmation",
        ActionClass::CredentialChange => "Credential and security changes require confirmation",
        ActionClass::IrreversibleDeletion => {
            "Irreversible deletion without a recovery path requires confirmation"
        }
        ActionClass::PrivilegedSystem => {
            "Privileged changes must pass through an allowlisted polkit service"
        }
    };
    let decision = match class {
        ActionClass::Normal => PermissionDecision::Allowed,
        ActionClass::PrivilegedSystem => PermissionDecision::Denied,
        _ if explicitly_confirmed => PermissionDecision::Allowed,
        _ => PermissionDecision::ConfirmationRequired,
    };
    PolicyEvaluation {
        decision,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_actions_are_autonomous() {
        assert_eq!(
            evaluate(ActionClass::Normal, false).decision,
            PermissionDecision::Allowed
        );
    }

    #[test]
    fn external_sends_need_confirmation() {
        assert_eq!(
            evaluate(ActionClass::ExternalSend, false).decision,
            PermissionDecision::ConfirmationRequired
        );
        assert_eq!(
            evaluate(ActionClass::ExternalSend, true).decision,
            PermissionDecision::Allowed
        );
    }

    #[test]
    fn unrestricted_privilege_is_never_granted() {
        assert_eq!(
            evaluate(ActionClass::PrivilegedSystem, true).decision,
            PermissionDecision::Denied
        );
    }
}
