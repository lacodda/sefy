//! What a plugin says about itself, and what sefy makes of the answer.

use serde::{Deserialize, Serialize};

/// The protocol version this build speaks.
///
/// A plugin declaring anything else is listed but refused, with the mismatch
/// shown: an integration that silently fails to appear is indistinguishable
/// from one that was never installed.
pub const PROTOCOL_VERSION: u32 = 1;

/// Name a plugin executable must have to be discovered.
pub const PREFIX: &str = "sefy-plugin-";

/// What a plugin prints when run with `--manifest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Protocol the plugin speaks; must equal [`PROTOCOL_VERSION`] to be usable.
    pub protocol_version: u32,
    /// Short name of the transport, e.g. `github`.
    pub name: String,
    /// The plugin's own version, shown for diagnosis only.
    pub version: String,
    /// One line about what it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Operations the plugin implements.
    ///
    /// A transport that can only publish declares `push` alone; sefy then
    /// refuses `pull` on it with that reason rather than running it and
    /// interpreting the failure.
    #[serde(default)]
    pub operations: Vec<Operation>,
}

/// What sefy can ask a transport to do.
///
/// Deliberately two. A plugin moves one opaque file to a remote and back; every
/// question of *what changed* is answered by sefy itself, with the vault open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    /// Upload the local vault file, replacing whatever the remote holds.
    Push,
    /// Download the remote copy to a path sefy chose.
    Pull,
}

impl Operation {
    /// Name of the operation as it appears in messages and on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::Pull => "pull",
        }
    }
}

impl std::fmt::Display for Operation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A plugin found on this machine, whether or not it can be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plugin {
    /// Executable name, e.g. `sefy-plugin-github`.
    pub executable: String,
    /// Where the executable was found.
    pub path: std::path::PathBuf,
    /// `None` when the manifest could not be read; `reason` says why.
    pub manifest: Option<Manifest>,
    /// Whether sefy is willing to run it.
    pub usable: bool,
    /// Why it is unusable, phrased for the person reading `sefy plugin list`.
    pub reason: Option<String>,
}

impl Plugin {
    /// Short name from the manifest, falling back to the executable's suffix.
    ///
    /// A plugin too broken to describe itself still has to be addressable —
    /// otherwise it could not be named in the command that reports on it.
    pub fn name(&self) -> &str {
        match &self.manifest {
            Some(manifest) => &manifest.name,
            None => self
                .executable
                .strip_prefix(PREFIX)
                .unwrap_or(&self.executable),
        }
    }

    /// Whether the plugin declares this operation.
    pub fn supports(&self, operation: Operation) -> bool {
        self.manifest
            .as_ref()
            .is_some_and(|manifest| manifest.operations.contains(&operation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_parses_with_only_its_required_fields() {
        let raw = r#"{"protocol_version":1,"name":"demo","version":"0.1.0"}"#;

        let manifest: Manifest = serde_json::from_str(raw).unwrap();

        assert_eq!(manifest.protocol_version, 1);
        assert!(manifest.operations.is_empty());
    }

    #[test]
    fn operations_are_read_from_the_manifest() {
        let raw = r#"{"protocol_version":1,"name":"demo","version":"0.1.0",
                      "operations":["push","pull"]}"#;

        let manifest: Manifest = serde_json::from_str(raw).unwrap();

        assert_eq!(manifest.operations, vec![Operation::Push, Operation::Pull]);
    }

    #[test]
    fn an_unknown_operation_is_refused_rather_than_guessed() {
        let raw = r#"{"protocol_version":1,"name":"demo","version":"0.1.0",
                      "operations":["teleport"]}"#;

        assert!(serde_json::from_str::<Manifest>(raw).is_err());
    }

    #[test]
    fn a_plugin_without_a_manifest_is_still_named() {
        let plugin = Plugin {
            executable: "sefy-plugin-broken".into(),
            path: "/somewhere/sefy-plugin-broken".into(),
            manifest: None,
            usable: false,
            reason: Some("nope".into()),
        };

        assert_eq!(plugin.name(), "broken");
    }

    #[test]
    fn an_undeclared_operation_is_not_supported() {
        let plugin = Plugin {
            executable: "sefy-plugin-demo".into(),
            path: "/somewhere/sefy-plugin-demo".into(),
            manifest: Some(Manifest {
                protocol_version: PROTOCOL_VERSION,
                name: "demo".into(),
                version: "0.1.0".into(),
                description: None,
                operations: vec![Operation::Push],
            }),
            usable: true,
            reason: None,
        };

        assert!(plugin.supports(Operation::Push));
        assert!(!plugin.supports(Operation::Pull));
    }
}
