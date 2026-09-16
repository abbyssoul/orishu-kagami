//! Deterministic, transport-neutral provider resolution over verified releases.
//!
//! Installation, availability, explicit selection and executable admission are
//! different states. This module chooses no physics by installation order. It
//! neither performs IO nor executes kernels; a successful selection still needs
//! workload compilation, independent admission and numerical validation.
//!
//! Eligibility is calculated from this request's explicit selections plus the
//! snapshot's enabled default releases. It does not expand as traversal proceeds.
//! Multiple exact candidates require a choice; this resolver does not search for
//! the first candidate whose dependency graph happens to succeed. Inspecting a
//! candidate's own resolution can explain its transitive requirements to a UI.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::*;

/// Owned verified declaration closure, without storing artifact bytes.
/// Only verification constructs this value; callers cannot mutate accepted data.
#[derive(Clone, Debug)]
pub struct VerifiedRelease {
    id: PluginReleaseId,
    root: Release,
    payloads: BTreeMap<LocalContributionId, VerifiedPayload>,
    contracts: BTreeMap<LocalContributionId, ScientificContractRef>,
}

impl VerifiedRelease {
    /// Verify all declared bytes and payloads before publishing a release.
    /// Artifacts remain in caller-owned storage. This does not inspect Wasm ABI.
    pub fn verify(
        root: Release,
        blobs: &BTreeMap<ArtifactDigest, &[u8]>,
        limits: &Limits,
    ) -> Result<Self, Error> {
        let payloads = root.validate(limits)?.verify_all(blobs)?;
        let contracts = payloads
            .iter()
            .filter_map(|(id, p)| {
                p.payload()
                    .map(|p| p.contract_ref(limits).map(|c| (id.clone(), c)))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            id: root.release_id(limits)?,
            root,
            payloads,
            contracts,
        })
    }

    /// Immutable content identity.
    pub fn id(&self) -> PluginReleaseId {
        self.id
    }
    /// Verified root declarations.
    pub fn root(&self) -> &Release {
        &self.root
    }
    /// All verified contribution payloads, including opaque future versions.
    pub fn payloads(&self) -> &BTreeMap<LocalContributionId, VerifiedPayload> {
        &self.payloads
    }
    /// Provider-qualified reference for an existing contribution.
    pub fn contribution_ref(&self, local: &LocalContributionId) -> Option<ContributionRef> {
        self.payloads.get(local).map(|p| ContributionRef {
            release: self.id,
            extension_point: p.extension_point().clone(),
            local_id: local.clone(),
        })
    }
}

/// Caller-owned work/count budgets, independent of immutable release identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolutionLimits {
    /// Installed releases in one snapshot.
    pub max_releases: usize,
    /// Total installed contributions.
    pub max_contributions: usize,
    /// Explicit experiment roots.
    pub max_roots: usize,
    /// Persisted bindings and produced bindings, independently bounded.
    pub max_bindings: usize,
    /// Longest active dependency path; additionally capped at 64 for stack safety.
    pub max_dependency_depth: usize,
    /// Total visited nodes, inspected candidates and dependency edges per request.
    pub max_work: usize,
    /// Maximum diagnostics; truncation is explicit and never counts as success.
    pub max_diagnostics: usize,
    /// Candidates included in one ambiguity diagnostic (full count is retained).
    pub max_candidates: usize,
}

impl Default for ResolutionLimits {
    fn default() -> Self {
        Self {
            max_releases: 256,
            max_contributions: 4096,
            max_roots: 4096,
            max_bindings: 4096,
            max_dependency_depth: 32,
            max_work: 1_000_000,
            max_diagnostics: 64,
            max_candidates: 32,
        }
    }
}

/// One installed release's session-effective enablement and logical default.
/// All releases of one plugin must agree on logical enablement, and exactly one
/// installed release of each plugin is its default. Process-only overrides are
/// applied by the inventory owner before constructing this snapshot.
#[derive(Clone, Copy, Debug)]
pub struct InventoryEntry<'a> {
    /// Verified immutable release.
    pub release: &'a VerifiedRelease,
    /// Logical plugin enablement, not per-contribution selection.
    pub enabled: bool,
    /// Whether this is the logical plugin's default release.
    pub is_default: bool,
}

/// A validated inventory revision, borrowed from an IO owner's immutable releases.
#[derive(Debug)]
pub struct Inventory<'a> {
    revision: u64,
    releases: BTreeMap<PluginReleaseId, InventoryEntry<'a>>,
    providers: BTreeMap<ScientificContractRef, Vec<ContributionRef>>,
    limits: ResolutionLimits,
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorCode::InvalidSelection, "inventory", message)
}
fn over(path: &str) -> Error {
    Error::new(ErrorCode::LimitExceeded, path, "resolution budget exceeded")
}

impl<'a> Inventory<'a> {
    /// Construct a deterministic snapshot. Duplicate releases, conflicting logical
    /// enablement or missing/multiple defaults are errors, never last-write-wins.
    pub fn new(
        revision: u64,
        entries: &[InventoryEntry<'a>],
        limits: ResolutionLimits,
    ) -> Result<Self, Error> {
        if entries.len() > limits.max_releases {
            return Err(over("releases"));
        }
        let mut releases = BTreeMap::new();
        let mut plugins = BTreeMap::<&PluginId, (bool, usize)>::new();
        let mut providers = BTreeMap::<_, Vec<_>>::new();
        let mut total = 0usize;
        for &entry in entries {
            let r = entry.release;
            total = total
                .checked_add(r.payloads.len())
                .filter(|n| *n <= limits.max_contributions)
                .ok_or_else(|| over("contributions"))?;
            if releases.insert(r.id, entry).is_some() {
                return Err(invalid("duplicate installed release"));
            }
            let state = plugins
                .entry(&r.root.0.metadata.plugin_id)
                .or_insert((entry.enabled, 0));
            if state.0 != entry.enabled {
                return Err(invalid("logical enablement differs across releases"));
            }
            state.1 += usize::from(entry.is_default);
            for (local, contract) in &r.contracts {
                providers
                    .entry(contract.clone())
                    .or_default()
                    .push(r.contribution_ref(local).expect("verified contribution"));
            }
        }
        if plugins.values().any(|(_, defaults)| *defaults != 1) {
            return Err(invalid(
                "each logical plugin needs exactly one default release",
            ));
        }
        for candidates in providers.values_mut() {
            candidates.sort();
        }
        Ok(Self {
            revision,
            releases,
            providers,
            limits,
        })
    }

    /// Snapshot identity against which responses and candidate lists were computed.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Page all exact eligible alternatives for an external requirement, using
    /// the same explicit-selection/default-release policy as `resolve`. A page
    /// never selects a provider. Offsets are meaningful only for this immutable
    /// inventory revision and the unchanged request/requirement.
    pub fn candidate_page(
        &self,
        request: &ResolutionRequest,
        requirement: &RequirementKey,
        offset: usize,
    ) -> Result<CandidatePageResponse, Error> {
        if request.roots.len() > self.limits.max_roots
            || request.bindings.len() > self.limits.max_bindings
        {
            return Err(over("request"));
        }
        if request.expected_inventory_revision != self.revision {
            return Ok(CandidatePageResponse::StaleRevision {
                expected: request.expected_inventory_revision,
                actual: self.revision,
            });
        }
        if self.limits.max_candidates == 0 || offset > self.limits.max_contributions {
            return Err(over("candidate page"));
        }
        let mut explicit: BTreeSet<_> = request.roots.iter().cloned().collect();
        if explicit.len() != request.roots.len() {
            return Err(invalid("duplicate root selection"));
        }
        let mut keys = BTreeSet::new();
        for binding in &request.bindings {
            if !keys.insert(&binding.requirement) {
                return Err(invalid("duplicate requirement binding"));
            }
            explicit.insert(binding.provider.clone());
        }
        let consumer = self
            .releases
            .get(&requirement.consumer.release)
            .and_then(|e| {
                e.release.root.0.spec.contributions.iter().find(|c| {
                    c.local_id == requirement.consumer.local_id
                        && c.extension_point == requirement.consumer.extension_point
                })
            })
            .ok_or_else(|| invalid("unknown candidate-page consumer"))?;
        let Some(Requirement::Contract { contract, .. }) = consumer
            .requirements
            .iter()
            .find(|r| r.slot() == &requirement.slot)
        else {
            return Err(invalid(
                "candidate pages require an external scientific requirement",
            ));
        };
        let mut work = self.limits.max_work;
        let mut total = 0usize;
        let mut candidates = Vec::new();
        for provider in self.providers.get(contract).into_iter().flatten() {
            work = work.checked_sub(1).ok_or_else(|| over("candidate page"))?;
            if self.eligible(provider, &explicit) {
                if total >= offset && candidates.len() < self.limits.max_candidates {
                    candidates.push(provider.clone());
                }
                total += 1;
            }
        }
        if offset > total {
            return Err(invalid("candidate page offset exceeds candidate count"));
        }
        let end = offset + candidates.len();
        Ok(CandidatePageResponse::Page {
            inventory_revision: self.revision,
            candidates,
            total,
            next_offset: (end < total).then_some(end),
        })
    }

    fn eligible(&self, provider: &ContributionRef, explicit: &BTreeSet<ContributionRef>) -> bool {
        let entry = &self.releases[&provider.release];
        entry.enabled && (entry.is_default || explicit.contains(provider))
    }

    /// Resolve an experiment's roots and persisted provider bindings atomically.
    /// Nothing is installed or modified. Failure returns diagnostics, never a
    /// partly accepted replacement selection. Retry with a fresh revision after an
    /// inventory change; an old candidate list cannot authorize a new selection.
    pub fn resolve(&self, request: &ResolutionRequest) -> Result<ResolutionOutcome, Error> {
        if request.roots.len() > self.limits.max_roots {
            return Err(over("roots"));
        }
        if request.bindings.len() > self.limits.max_bindings {
            return Err(over("bindings"));
        }
        if request.expected_inventory_revision != self.revision {
            return Ok(ResolutionOutcome::StaleRevision {
                expected: request.expected_inventory_revision,
                actual: self.revision,
            });
        }
        let roots: BTreeSet<_> = request.roots.iter().cloned().collect();
        if roots.len() != request.roots.len() {
            return Err(invalid("duplicate root selection"));
        }
        let mut pins = BTreeMap::new();
        let mut selected = roots.clone();
        for b in &request.bindings {
            if pins
                .insert(b.requirement.clone(), b.provider.clone())
                .is_some()
            {
                return Err(invalid("duplicate requirement binding"));
            }
            selected.insert(b.provider.clone());
        }
        let mut resolver = Resolver {
            inventory: self,
            pins,
            explicit: selected,
            path: Vec::new(),
            visited: BTreeMap::new(),
            output: BTreeMap::new(),
            used_pins: BTreeSet::new(),
            issues: Vec::new(),
            truncated: false,
            remaining: self.limits.max_work,
            exhausted: false,
        };
        for root in &roots {
            if resolver.exhausted {
                break;
            }
            resolver.visit(root, None);
        }
        if !resolver.exhausted {
            // A stale/unreachable binding must not silently grant eligibility to
            // an unrelated provider. It is retained as authored intent on failure.
            let unused: Vec<_> = resolver
                .pins
                .keys()
                .filter(|k| !resolver.used_pins.contains(*k))
                .cloned()
                .collect();
            for key in unused {
                resolver.issue(ResolutionIssue {
                    contribution: key.consumer.clone(),
                    requirement: Some(key),
                    reason: UnavailableReason::UnusedBinding,
                    candidates: vec![],
                    candidate_count: 0,
                });
            }
        }
        if !resolver.issues.is_empty() || resolver.truncated || resolver.exhausted {
            return Ok(ResolutionOutcome::Unavailable {
                inventory_revision: self.revision,
                issues: resolver.issues,
                truncated: resolver.truncated,
            });
        }
        Ok(ResolutionOutcome::Resolved {
            inventory_revision: self.revision,
            selection: Selection {
                roots: roots.into_iter().collect(),
                contributions: resolver.visited.into_keys().collect(),
                bindings: resolver
                    .output
                    .into_iter()
                    .map(|(requirement, provider)| ProviderBinding {
                        requirement,
                        provider,
                    })
                    .collect(),
            },
        })
    }
}

/// Identity of one dependency slot on one exact contribution.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequirementKey {
    /// Exact consuming contribution.
    pub consumer: ContributionRef,
    /// Release-local requirement slot.
    pub slot: LocalContributionId,
}

/// Persistable explicit choice; validity is always rechecked against a snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderBinding {
    /// Slot being bound.
    pub requirement: RequirementKey,
    /// Exact selected provider; no mutable label or fallback.
    pub provider: ContributionRef,
}

/// Raw authoring intent. Adapters bound wire input before deserializing; the pure
/// resolver independently bounds all collections and work before using this value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolutionRequest {
    /// Inventory revision used to author this request.
    pub expected_inventory_revision: u64,
    /// Explicit experiment contributions, including non-default pinned releases.
    pub roots: Vec<ContributionRef>,
    /// Persisted choices and explicit responses to ambiguity diagnostics.
    pub bindings: Vec<ProviderBinding>,
}

/// A complete selection, serialized inside a versioned authoring resource by its
/// owner. Deserializing it does not grant admission; reconstruct a request and
/// resolve/validate it again. This is not the workload selection evidence format.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    /// Original explicitly selected roots, sorted by exact reference.
    pub roots: Vec<ContributionRef>,
    /// Complete transitive contribution set, sorted by exact reference.
    pub contributions: Vec<ContributionRef>,
    /// Every resolved dependency slot, including same-release dependencies.
    pub bindings: Vec<ProviderBinding>,
}

/// Structured response; no interactive prompting occurs in this core.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase", deny_unknown_fields)]
pub enum ResolutionOutcome {
    /// All dependencies resolved with no missing choices.
    Resolved {
        /// Snapshot identity.
        #[serde(rename = "inventoryRevision")]
        inventory_revision: u64,
        /// Complete pinned selection, suitable for an atomic authoring command.
        selection: Selection,
    },
    /// Resolution failed; authored intent remains untouched.
    Unavailable {
        /// Snapshot identity, also governing returned candidate lists.
        #[serde(rename = "inventoryRevision")]
        inventory_revision: u64,
        /// Bounded, deterministically ordered diagnostic details.
        issues: Vec<ResolutionIssue>,
        /// More diagnostics existed than fit the caller's budget.
        truncated: bool,
    },
    /// A request/candidate list refers to an obsolete inventory revision.
    StaleRevision {
        /// Revision supplied by caller.
        expected: u64,
        /// Current snapshot revision.
        actual: u64,
    },
}

/// A direct failure, attributed to the contribution and dependency slot involved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolutionIssue {
    /// Contribution being inspected.
    pub contribution: ContributionRef,
    /// Parent dependency slot, absent for an explicit root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirement: Option<RequirementKey>,
    /// Structured cause.
    pub reason: UnavailableReason,
    /// Bounded exact choices; ordering is independent of installation order.
    pub candidates: Vec<ContributionRef>,
    /// Full eligible-candidate count; larger than `candidates.len()` means truncated.
    pub candidate_count: usize,
}

/// Revision-bound candidate pagination, without automatic provider selection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase", deny_unknown_fields)]
pub enum CandidatePageResponse {
    /// One bounded slice of stable exact references.
    Page {
        /// Revision governing this list.
        #[serde(rename = "inventoryRevision")]
        inventory_revision: u64,
        /// Exact choices in deterministic order.
        candidates: Vec<ContributionRef>,
        /// Total eligible choices.
        total: usize,
        /// Next offset, absent after the last page.
        #[serde(rename = "nextOffset", skip_serializing_if = "Option::is_none")]
        next_offset: Option<usize>,
    },
    /// Refresh the inventory and retry; an old page must not authorize a choice.
    StaleRevision {
        /// Requested revision.
        expected: u64,
        /// Available revision.
        actual: u64,
    },
}

/// Why a contribution or binding could not become a complete selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnavailableReason {
    /// Referenced release is not installed.
    MissingRelease,
    /// Referenced contribution/extension point is not in that release.
    MissingContribution,
    /// Its logical plugin is disabled for this session.
    Disabled,
    /// Its extension point/version is not understood.
    UnsupportedContribution,
    /// No enabled eligible contribution supplies this exact scientific contract.
    MissingProvider,
    /// More than one exact eligible provider; caller must choose.
    AmbiguousProvider,
    /// Existing pin does not supply the declared exact contract/local target.
    IncompatiblePin,
    /// Dependency recursion revisited an active contribution.
    DependencyCycle,
    /// This declared binding has no reachable dependency slot.
    UnusedBinding,
    /// Some transitive dependency failed; no partial selection was accepted.
    DependencyUnavailable,
    /// Caller work, output or depth budget prevented complete resolution.
    LimitExceeded,
}

struct Resolver<'a, 'r> {
    inventory: &'a Inventory<'r>,
    pins: BTreeMap<RequirementKey, ContributionRef>,
    explicit: BTreeSet<ContributionRef>,
    path: Vec<ContributionRef>,
    visited: BTreeMap<ContributionRef, bool>,
    output: BTreeMap<RequirementKey, ContributionRef>,
    used_pins: BTreeSet<RequirementKey>,
    issues: Vec<ResolutionIssue>,
    truncated: bool,
    remaining: usize,
    exhausted: bool,
}

impl Resolver<'_, '_> {
    fn issue(&mut self, issue: ResolutionIssue) {
        if self.issues.len() < self.inventory.limits.max_diagnostics {
            self.issues.push(issue);
        } else {
            self.truncated = true;
        }
    }
    fn fail(
        &mut self,
        c: &ContributionRef,
        requirement: Option<&RequirementKey>,
        reason: UnavailableReason,
    ) -> bool {
        self.issue(ResolutionIssue {
            contribution: c.clone(),
            requirement: requirement.cloned(),
            reason,
            candidates: vec![],
            candidate_count: 0,
        });
        false
    }
    fn spend(&mut self, c: &ContributionRef, key: Option<&RequirementKey>) -> bool {
        if let Some(n) = self.remaining.checked_sub(1) {
            self.remaining = n;
            true
        } else {
            if !self.exhausted {
                self.fail(c, key, UnavailableReason::LimitExceeded);
            }
            self.exhausted = true;
            false
        }
    }
    fn entry(
        &self,
        c: &ContributionRef,
    ) -> Result<(&VerifiedRelease, &VerifiedPayload), UnavailableReason> {
        let entry = self
            .inventory
            .releases
            .get(&c.release)
            .ok_or(UnavailableReason::MissingRelease)?;
        if !entry.enabled {
            return Err(UnavailableReason::Disabled);
        }
        let p = entry
            .release
            .payloads
            .get(&c.local_id)
            .filter(|p| p.extension_point() == &c.extension_point)
            .ok_or(UnavailableReason::MissingContribution)?;
        if p.payload().is_none() {
            return Err(UnavailableReason::UnsupportedContribution);
        }
        Ok((entry.release, p))
    }
    fn visit(&mut self, c: &ContributionRef, parent: Option<&RequirementKey>) -> bool {
        if !self.spend(c, parent) {
            return false;
        }
        if self.path.contains(c) {
            return self.fail(c, parent, UnavailableReason::DependencyCycle);
        }
        if let Some(&success) = self.visited.get(c) {
            return success || self.fail(c, parent, UnavailableReason::DependencyUnavailable);
        }
        if self.path.len() >= self.inventory.limits.max_dependency_depth.min(64) {
            return self.fail(c, parent, UnavailableReason::LimitExceeded);
        }
        let (release, _) = match self.entry(c) {
            Ok(e) => e,
            Err(reason) => return self.fail(c, parent, reason),
        };
        // Clone at most the manifest's bounded requirements, not a payload/blob.
        let mut requirements = release
            .root
            .0
            .spec
            .contributions
            .iter()
            .find(|v| v.local_id == c.local_id)
            .expect("verified root contribution")
            .requirements
            .clone();
        requirements.sort_by(|a, b| a.slot().cmp(b.slot()));
        self.path.push(c.clone());
        let mut success = true;
        for requirement in requirements {
            let key = RequirementKey {
                consumer: c.clone(),
                slot: requirement.slot().clone(),
            };
            if !self.spend(c, Some(&key)) {
                success = false;
                break;
            }
            let Some(provider) = self.provider(c, &key, &requirement) else {
                success = false;
                continue;
            };
            if self.output.len() >= self.inventory.limits.max_bindings {
                self.fail(c, Some(&key), UnavailableReason::LimitExceeded);
                success = false;
                self.exhausted = true;
                break;
            }
            self.output.insert(key.clone(), provider.clone());
            success &= self.visit(&provider, Some(&key));
            if self.exhausted {
                break;
            }
        }
        self.path.pop();
        self.visited.insert(c.clone(), success);
        success
    }
    fn provider(
        &mut self,
        c: &ContributionRef,
        key: &RequirementKey,
        requirement: &Requirement,
    ) -> Option<ContributionRef> {
        let local = match requirement {
            Requirement::Local {
                local_contribution, ..
            } => Some(
                self.inventory.releases[&c.release]
                    .release
                    .contribution_ref(local_contribution)
                    .expect("verified local dependency"),
            ),
            _ => None,
        };
        if let Some(pin) = self.pins.get(key).cloned() {
            self.used_pins.insert(key.clone());
            let (release, _) = match self.entry(&pin) {
                Ok(e) => e,
                Err(reason) => {
                    self.fail(&pin, Some(key), reason);
                    return None;
                }
            };
            let matches = match requirement {
                Requirement::Local { .. } => local.as_ref() == Some(&pin),
                Requirement::Contract { contract, .. } => {
                    release.contracts.get(&pin.local_id) == Some(contract)
                }
            };
            if !matches {
                self.fail(&pin, Some(key), UnavailableReason::IncompatiblePin);
                return None;
            }
            return Some(pin);
        }
        if local.is_some() {
            return local;
        }
        let Requirement::Contract { contract, .. } = requirement else {
            unreachable!()
        };
        let mut candidates = Vec::new();
        let mut count = 0;
        let mut only = None;
        if let Some(providers) = self.inventory.providers.get(contract) {
            for p in providers {
                if !self.spend(c, Some(key)) {
                    return None;
                }
                if self.inventory.eligible(p, &self.explicit) {
                    count += 1;
                    only = Some(p.clone());
                    if candidates.len() < self.inventory.limits.max_candidates {
                        candidates.push(p.clone());
                    }
                }
            }
        }
        if count == 1 {
            return only;
        }
        self.issue(ResolutionIssue {
            contribution: c.clone(),
            requirement: Some(key.clone()),
            reason: if count == 0 {
                UnavailableReason::MissingProvider
            } else {
                UnavailableReason::AmbiguousProvider
            },
            candidates,
            candidate_count: count,
        });
        None
    }
}
