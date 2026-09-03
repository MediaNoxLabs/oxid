// SPDX-License-Identifier: Apache-2.0

//! Bounded public projection of one compile-time standalone deployment.

use std::sync::Arc;

/// The only standalone route families the application may identify.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeploymentRouteClass {
    Local,
    Tailnet,
}

impl DeploymentRouteClass {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Tailnet => "tailnet",
        }
    }
}

/// The closed set of development deployment profiles selectable by a build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandaloneDeploymentProfile {
    Local,
    Tailnet,
}

impl StandaloneDeploymentProfile {
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        match self {
            Self::Local => "standalone-local",
            Self::Tailnet => "standalone-tailnet",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Local => "Standalone · local",
            Self::Tailnet => "Standalone · Tailnet",
        }
    }

    #[must_use]
    pub const fn network_id(self) -> &'static str {
        "undeployed"
    }

    #[must_use]
    pub const fn route_class(self) -> DeploymentRouteClass {
        match self {
            Self::Local => DeploymentRouteClass::Local,
            Self::Tailnet => DeploymentRouteClass::Tailnet,
        }
    }
}

/// Sanitized result of one bounded transport probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeploymentServiceReadiness {
    Ready,
    Unavailable,
    NotConfigured,
}

impl DeploymentServiceReadiness {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Unavailable => "unavailable",
            Self::NotConfigured => "not_configured",
        }
    }
}

/// Independent readiness values. It deliberately has no aggregate state so a
/// healthy service cannot conceal a failed sibling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeploymentServiceSnapshot {
    indexer: DeploymentServiceReadiness,
    node: DeploymentServiceReadiness,
    prover: DeploymentServiceReadiness,
    ssi: DeploymentServiceReadiness,
}

impl DeploymentServiceSnapshot {
    #[must_use]
    pub const fn new(
        indexer: DeploymentServiceReadiness,
        node: DeploymentServiceReadiness,
        prover: DeploymentServiceReadiness,
        ssi: DeploymentServiceReadiness,
    ) -> Self {
        Self {
            indexer,
            node,
            prover,
            ssi,
        }
    }

    #[must_use]
    pub const fn unavailable(ssi_configured: bool) -> Self {
        Self::new(
            DeploymentServiceReadiness::Unavailable,
            DeploymentServiceReadiness::Unavailable,
            DeploymentServiceReadiness::Unavailable,
            if ssi_configured {
                DeploymentServiceReadiness::Unavailable
            } else {
                DeploymentServiceReadiness::NotConfigured
            },
        )
    }

    #[must_use]
    pub const fn indexer(self) -> DeploymentServiceReadiness {
        self.indexer
    }

    #[must_use]
    pub const fn node(self) -> DeploymentServiceReadiness {
        self.node
    }

    #[must_use]
    pub const fn prover(self) -> DeploymentServiceReadiness {
        self.prover
    }

    #[must_use]
    pub const fn ssi(self) -> DeploymentServiceReadiness {
        self.ssi
    }
}

/// Outgoing boundary implemented by the transport adapter. Implementations
/// must discard endpoint and transport details before returning.
pub trait DeploymentReadinessPort: Send + Sync {
    fn inspect(&self) -> DeploymentServiceSnapshot;
}

/// Public view consumed by incoming adapters. URLs, peer identities, response
/// bodies, and transport errors cannot be represented by this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeploymentProfileView {
    profile_id: &'static str,
    label: &'static str,
    network_id: &'static str,
    route_class: DeploymentRouteClass,
    services: DeploymentServiceSnapshot,
}

impl DeploymentProfileView {
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.profile_id
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        self.label
    }

    #[must_use]
    pub const fn network_id(self) -> &'static str {
        self.network_id
    }

    #[must_use]
    pub const fn route_class(self) -> DeploymentRouteClass {
        self.route_class
    }

    #[must_use]
    pub const fn services(self) -> DeploymentServiceSnapshot {
        self.services
    }
}

/// Incoming boundary for one owner-requested readiness refresh.
pub trait GetDeploymentProfileUseCase: Send + Sync {
    fn execute(&self) -> DeploymentProfileView;
}

/// Joins a closed compile-time identity to sanitized adapter readiness.
pub struct DeploymentProfileService {
    profile: StandaloneDeploymentProfile,
    readiness: Arc<dyn DeploymentReadinessPort>,
}

impl DeploymentProfileService {
    #[must_use]
    pub const fn new(
        profile: StandaloneDeploymentProfile,
        readiness: Arc<dyn DeploymentReadinessPort>,
    ) -> Self {
        Self { profile, readiness }
    }
}

impl GetDeploymentProfileUseCase for DeploymentProfileService {
    fn execute(&self) -> DeploymentProfileView {
        DeploymentProfileView {
            profile_id: self.profile.profile_id(),
            label: self.profile.label(),
            network_id: self.profile.network_id(),
            route_class: self.profile.route_class(),
            services: self.readiness.inspect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedReadiness;

    impl DeploymentReadinessPort for FixedReadiness {
        fn inspect(&self) -> DeploymentServiceSnapshot {
            DeploymentServiceSnapshot::new(
                DeploymentServiceReadiness::Ready,
                DeploymentServiceReadiness::Unavailable,
                DeploymentServiceReadiness::Ready,
                DeploymentServiceReadiness::NotConfigured,
            )
        }
    }

    #[test]
    fn projection_contains_only_closed_identity_and_readiness_values() {
        let service = DeploymentProfileService::new(
            StandaloneDeploymentProfile::Tailnet,
            Arc::new(FixedReadiness),
        );

        let view = service.execute();

        assert_eq!(view.profile_id(), "standalone-tailnet");
        assert_eq!(view.label(), "Standalone · Tailnet");
        assert_eq!(view.network_id(), "undeployed");
        assert_eq!(view.route_class().as_str(), "tailnet");
        assert_eq!(view.services().indexer().as_str(), "ready");
        assert_eq!(view.services().node().as_str(), "unavailable");
        assert_eq!(view.services().ssi().as_str(), "not_configured");
    }

    #[test]
    fn unavailable_snapshot_preserves_optional_ssi_shape() {
        assert_eq!(
            DeploymentServiceSnapshot::unavailable(false).ssi(),
            DeploymentServiceReadiness::NotConfigured
        );
        assert_eq!(
            DeploymentServiceSnapshot::unavailable(true).ssi(),
            DeploymentServiceReadiness::Unavailable
        );
    }
}
