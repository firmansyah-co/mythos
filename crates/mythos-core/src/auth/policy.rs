//! Which account may call which route.
//!
//! Every route of the API is listed here by its method and its pattern as the router declares
//! it. A route that is not listed is for administrators only: a new route is closed until
//! someone decides who may use it, and `every_route_has_a_policy` fails until they do.

use super::Role;

/// What a route requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Anyone, signed in or not: signing in itself, and the health check.
    Public,
    /// Any signed-in account, whatever its role: its own account and session.
    SignedIn,
    /// An account with at least this role.
    Role(Role),
}

const V: Access = Access::Role(Role::Viewer);
const E: Access = Access::Role(Role::Engineer);
const A: Access = Access::Role(Role::Admin);

/// `(method, route pattern, access)` for every route.
pub const POLICY: &[(&str, &str, Access)] = &[
    ("GET", "/api/v1/health", Access::Public),
    // Signing in
    ("GET", "/api/v1/auth/status", Access::Public),
    ("POST", "/api/v1/auth/setup", Access::Public),
    ("POST", "/api/v1/auth/login", Access::Public),
    ("POST", "/api/v1/auth/logout", Access::SignedIn),
    ("GET", "/api/v1/auth/me", Access::SignedIn),
    ("PUT", "/api/v1/auth/me/password", Access::SignedIn),
    ("GET", "/api/v1/auth/me/tokens", Access::SignedIn),
    ("POST", "/api/v1/auth/me/tokens", Access::SignedIn),
    ("DELETE", "/api/v1/auth/me/tokens/:token", Access::SignedIn),
    // Accounts
    ("GET", "/api/v1/users", A),
    ("POST", "/api/v1/users", A),
    ("PUT", "/api/v1/users/:username", A),
    ("DELETE", "/api/v1/users/:username", A),
    // Updates: anyone may see the versions; only an administrator changes them.
    // The node: anyone may see what it is; only an administrator renames it or changes its role.
    ("GET", "/api/v1/node", V),
    ("PUT", "/api/v1/node", A),
    // Plant projects (ADR-012 §2.4): an engineer changes the devices and downloads; a download
    // reaches a node as a slice, which that node accepts from an engineer (a paired node is
    // granted the role it was given).
    ("GET", "/api/v1/projects/:id/plant", V),
    ("PUT", "/api/v1/projects/:id/plant/devices/:device", E),
    ("DELETE", "/api/v1/projects/:id/plant/devices/:device", E),
    (
        "GET",
        "/api/v1/projects/:id/plant/devices/:device/compare",
        V,
    ),
    (
        "POST",
        "/api/v1/projects/:id/plant/devices/:device/download",
        E,
    ),
    ("POST", "/api/v1/projects/:id/plant/download", E),
    ("GET", "/api/v1/slices/:project", V),
    ("PUT", "/api/v1/slices/:project", E),
    // Pairing (ADR-012 §2.3): an administrator pairs and unpairs; another node pairs by the code
    // it was given, which is its only credential; a paired node says it unpaired, as itself.
    ("POST", "/api/v1/pairing/code", A),
    ("POST", "/api/v1/pairing/accept", Access::Public),
    ("DELETE", "/api/v1/pairing/peers/:node", Access::SignedIn),
    ("GET", "/api/v1/pairings", V),
    ("POST", "/api/v1/pairings", A),
    ("DELETE", "/api/v1/pairings/:node", A),
    ("GET", "/api/v1/pairings/:node/status", V),
    ("PUT", "/api/v1/pairings/:node", A),
    ("GET", "/api/v1/fleet", V),
    // Only a paired node, as itself: the handler takes the node from its pairing identity.
    ("POST", "/api/v1/callhome", Access::SignedIn),
    ("GET", "/api/v1/updates", V),
    ("GET", "/api/v1/updates/releases", V),
    ("PUT", "/api/v1/updates/follow", A),
    ("POST", "/api/v1/updates/install", A),
    ("POST", "/api/v1/updates/rollback", A),
    // Reading the controller
    ("GET", "/api/v1/tags", V),
    ("GET", "/api/v1/tags/values", V),
    ("GET", "/api/v1/remote", V),
    ("GET", "/api/v1/tags/:uri", V),
    ("GET", "/api/v1/projects", V),
    ("GET", "/api/v1/projects/active", V),
    ("GET", "/api/v1/projects/:id", V),
    ("GET", "/api/v1/projects/:id/datatypes", V),
    ("GET", "/api/v1/projects/:id/datatypes/:dut", V),
    ("GET", "/api/v1/projects/:id/io", V),
    ("GET", "/api/v1/telemetry/stream", V),
    ("GET", "/api/v1/io/status", V),
    ("GET", "/api/v1/io/drivers", V),
    ("GET", "/api/v1/modules", V),
    ("GET", "/modules/:module/ui/*path", V),
    // Settings → Modules: an executable put on the controller is an administrator's decision.
    ("POST", "/api/v1/modules/install", A),
    ("POST", "/api/v1/modules/install-url", A),
    ("PUT", "/api/v1/modules/:module/enabled", A),
    ("DELETE", "/api/v1/modules/:module", A),
    // Engineering
    ("POST", "/api/v1/projects", E),
    ("POST", "/api/v1/projects/:id/activate", E),
    ("POST", "/api/v1/projects/:id/datatypes", E),
    ("DELETE", "/api/v1/projects/:id/datatypes/:dut", E),
    ("PUT", "/api/v1/projects/:id/datatypes/:dut/draft", E),
    ("POST", "/api/v1/projects/:id/datatypes/:dut/release", E),
    ("PUT", "/api/v1/projects/:id/io", E),
    ("POST", "/api/v1/modules/rescan", E),
    ("POST", "/api/v1/control/deploy", E),
    // A service module's routes: any signed-in account gets as far as the module host, which
    // then requires the role the module's manifest declares for the route (crate::services).
    (
        "GET",
        "/api/v1/projects/:id/:module/*rest",
        Access::SignedIn,
    ),
    (
        "POST",
        "/api/v1/projects/:id/:module/*rest",
        Access::SignedIn,
    ),
    (
        "PUT",
        "/api/v1/projects/:id/:module/*rest",
        Access::SignedIn,
    ),
    (
        "DELETE",
        "/api/v1/projects/:id/:module/*rest",
        Access::SignedIn,
    ),
    ("GET", "/api/v1/m/:module/*rest", Access::SignedIn),
    ("POST", "/api/v1/m/:module/*rest", Access::SignedIn),
    ("PUT", "/api/v1/m/:module/*rest", Access::SignedIn),
    ("DELETE", "/api/v1/m/:module/*rest", Access::SignedIn),
    // Editing together: the collaboration room changes what it edits.
    ("GET", "/api/v1/collab/ws/:room_id", E),
];

/// What `method pattern` requires. Unlisted is administrators only.
pub fn required_access(method: &str, pattern: &str) -> Access {
    POLICY
        .iter()
        .find(|(m, p, _)| m.eq_ignore_ascii_case(method) && *p == pattern)
        .map(|(_, _, access)| *access)
        .unwrap_or(A)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unlisted_route_is_for_administrators_only() {
        assert_eq!(required_access("POST", "/api/v1/something/new"), A);
        assert_eq!(required_access("GET", "/api/v1/something/new"), A);
    }

    #[test]
    fn a_route_is_listed_once() {
        let mut seen: Vec<(&str, &str)> = POLICY.iter().map(|(m, p, _)| (*m, *p)).collect();
        seen.sort();
        let before = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), before);
    }

    #[test]
    fn nothing_that_changes_the_controller_is_open_to_viewers() {
        for (method, path, access) in POLICY {
            // A module's route passes here as SignedIn; the module host then requires the role
            // its manifest declares (crate::services::forward, tested in service_module tests).
            let module_route = path.ends_with("/:module/*rest");
            if *method != "GET" && path.starts_with("/api/v1/projects") && !module_route {
                assert_eq!(*access, E, "{method} {path}");
            }
        }
    }
}
