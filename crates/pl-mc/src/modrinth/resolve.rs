//! Breadth-first resolution of required Modrinth dependencies.
//!
//! Each round batches the work: pinned versions in one `/versions` call, unpinned projects
//! through `/project/{id}/version` filtered by loader and game version (a few at a time).
//! A project is visited at most once, so cycles and diamonds terminate and nothing is
//! installed twice.

use std::collections::{HashMap, HashSet};

use futures::StreamExt;

use super::{
    ModrinthClient, ModrinthDependency, ModrinthDependencyConflict, ModrinthDependencyResolution,
    ModrinthDependencyType, ModrinthInstalled, ModrinthResolveTarget, ModrinthResolvedDependency,
    ModrinthUnresolvedDependency, ModrinthUnresolvedReason, ModrinthVersion, ModrinthVersionType,
};
use crate::error::{Error, Result};

/// Concurrent `/project/{id}/version` requests per round.
const PROJECT_LOOKUPS_IN_FLIGHT: usize = 4;

/// Newest release, else newest beta, else newest alpha (the API lists newest first).
pub(crate) fn pick_best<'a>(
    versions: &'a [ModrinthVersion],
    target: &ModrinthResolveTarget,
) -> Option<&'a ModrinthVersion> {
    let compatible: Vec<&ModrinthVersion> = versions.iter().filter(|v| target.accepts(v)).collect();
    [
        ModrinthVersionType::Release,
        ModrinthVersionType::Beta,
        ModrinthVersionType::Alpha,
    ]
    .iter()
    .find_map(|kind| compatible.iter().copied().find(|v| v.version_type == *kind))
    .or_else(|| compatible.first().copied())
}

struct Resolver<'a> {
    client: &'a ModrinthClient,
    target: &'a ModrinthResolveTarget,
    installed: &'a ModrinthInstalled,
    /// Projects already chosen, skipped or in flight (roots included).
    visited: HashSet<String>,
    result: ModrinthDependencyResolution,
}

impl Resolver<'_> {
    fn unresolved(
        &mut self,
        dep: &ModrinthDependency,
        project_id: Option<String>,
        required_by: &str,
        reason: ModrinthUnresolvedReason,
    ) {
        self.result.unresolved.push(ModrinthUnresolvedDependency {
            project_id: project_id.or_else(|| dep.project_id.clone()),
            version_id: dep.version_id.clone(),
            file_name: dep.file_name.clone(),
            title: None,
            required_by: required_by.to_owned(),
            reason,
        });
    }

    fn mark_installed(&mut self, project_id: &str) {
        if !self
            .result
            .already_installed
            .iter()
            .any(|p| p == project_id)
        {
            self.result.already_installed.push(project_id.to_owned());
        }
    }

    fn note_conflicts(&mut self, version: &ModrinthVersion) {
        for dep in &version.dependencies {
            if dep.dependency_type != ModrinthDependencyType::Incompatible {
                continue;
            }
            let Some(project_id) = &dep.project_id else {
                continue;
            };
            let known = self
                .result
                .conflicts
                .iter()
                .any(|c| &c.project_id == project_id && c.declared_by == version.project_id);
            if self.installed.project_ids.contains(project_id) && !known {
                self.result.conflicts.push(ModrinthDependencyConflict {
                    project_id: project_id.clone(),
                    title: None,
                    declared_by: version.project_id.clone(),
                });
            }
        }
    }

    async fn run(mut self, roots: &[ModrinthVersion]) -> Result<ModrinthDependencyResolution> {
        let mut frontier: Vec<(ModrinthDependency, String)> = Vec::new();
        for root in roots {
            self.visited.insert(root.project_id.clone());
        }
        for root in roots {
            self.note_conflicts(root);
            frontier.extend(
                root.required_dependencies()
                    .map(|dep| (dep.clone(), root.project_id.clone())),
            );
        }

        while !frontier.is_empty() {
            let mut pinned: Vec<(String, ModrinthDependency, String)> = Vec::new();
            let mut unpinned: Vec<(String, String)> = Vec::new();
            for (dep, by) in std::mem::take(&mut frontier) {
                if let Some(project_id) = &dep.project_id {
                    if self.installed.project_ids.contains(project_id) {
                        self.mark_installed(project_id);
                        continue;
                    }
                    if self.visited.contains(project_id) {
                        continue;
                    }
                }
                match (&dep.version_id, &dep.project_id) {
                    (Some(version_id), _) => pinned.push((version_id.clone(), dep.clone(), by)),
                    (None, Some(project_id)) => {
                        self.visited.insert(project_id.clone());
                        unpinned.push((project_id.clone(), by));
                    }
                    (None, None) => {
                        self.unresolved(&dep, None, &by, ModrinthUnresolvedReason::External)
                    }
                }
            }

            let mut chosen: Vec<(ModrinthVersion, String)> = Vec::new();

            // Pinned versions: one batch request. A pin that does not fit the target falls
            // back to the best compatible version of the same project.
            if !pinned.is_empty() {
                let ids: Vec<String> = pinned.iter().map(|(id, _, _)| id.clone()).collect();
                let fetched: HashMap<String, ModrinthVersion> = self
                    .client
                    .versions(&ids)
                    .await?
                    .into_iter()
                    .map(|v| (v.id.clone(), v))
                    .collect();
                for (version_id, dep, by) in pinned {
                    let Some(version) = fetched.get(&version_id) else {
                        self.unresolved(&dep, None, &by, ModrinthUnresolvedReason::NotFound);
                        continue;
                    };
                    let project_id = version.project_id.clone();
                    if self.installed.project_ids.contains(&project_id) {
                        self.mark_installed(&project_id);
                        continue;
                    }
                    if !self.visited.insert(project_id.clone()) {
                        continue;
                    }
                    if self.target.accepts(version) {
                        chosen.push((version.clone(), by));
                    } else {
                        unpinned.push((project_id, by));
                    }
                }
            }

            // Unpinned projects: filtered version lists, a few requests at a time.
            let (client, target) = (self.client, self.target);
            let lookups: Vec<(String, String, Result<Vec<ModrinthVersion>>)> =
                futures::stream::iter(unpinned)
                    .map(|(project_id, by)| async move {
                        let versions = client
                            .project_versions(&project_id, &target.loaders, &target.game_versions)
                            .await;
                        (project_id, by, versions)
                    })
                    .buffered(PROJECT_LOOKUPS_IN_FLIGHT)
                    .collect()
                    .await;
            for (project_id, by, versions) in lookups {
                let dep = ModrinthDependency {
                    version_id: None,
                    project_id: Some(project_id.clone()),
                    file_name: None,
                    dependency_type: ModrinthDependencyType::Required,
                };
                match versions {
                    Ok(versions) => match pick_best(&versions, self.target) {
                        Some(best) => chosen.push((best.clone(), by)),
                        None => self.unresolved(
                            &dep,
                            Some(project_id),
                            &by,
                            ModrinthUnresolvedReason::NoCompatibleVersion,
                        ),
                    },
                    Err(Error::NotFound(_)) => self.unresolved(
                        &dep,
                        Some(project_id),
                        &by,
                        ModrinthUnresolvedReason::NotFound,
                    ),
                    Err(error) => return Err(error),
                }
            }

            for (version, by) in chosen {
                if version.has_file_hash(&self.installed.hashes) {
                    self.mark_installed(&version.project_id);
                    continue;
                }
                self.note_conflicts(&version);
                frontier.extend(
                    version
                        .required_dependencies()
                        .map(|dep| (dep.clone(), version.project_id.clone())),
                );
                self.result.to_install.push(ModrinthResolvedDependency {
                    project_id: version.project_id.clone(),
                    title: None,
                    icon_url: None,
                    version,
                    required_by: by,
                });
            }
        }

        self.fill_titles().await;
        Ok(self.result)
    }

    /// Best effort: project titles and icons for the UI (one batch request).
    async fn fill_titles(&mut self) {
        let mut ids: Vec<String> = self
            .result
            .to_install
            .iter()
            .map(|d| d.project_id.clone())
            .chain(
                self.result
                    .unresolved
                    .iter()
                    .filter_map(|d| d.project_id.clone()),
            )
            .chain(self.result.conflicts.iter().map(|c| c.project_id.clone()))
            .collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return;
        }
        let projects = match self.client.projects(&ids).await {
            Ok(projects) => projects,
            Err(error) => {
                tracing::debug!(%error, "could not fetch dependency titles");
                return;
            }
        };
        let by_id: HashMap<&str, _> = projects.iter().map(|p| (p.id.as_str(), p)).collect();
        for dep in &mut self.result.to_install {
            if let Some(project) = by_id.get(dep.project_id.as_str()) {
                dep.title = Some(project.title.clone());
                dep.icon_url = project.icon_url.clone();
            }
        }
        for dep in &mut self.result.unresolved {
            if let Some(project) = dep.project_id.as_deref().and_then(|id| by_id.get(id)) {
                dep.title = Some(project.title.clone());
            }
        }
        for conflict in &mut self.result.conflicts {
            if let Some(project) = by_id.get(conflict.project_id.as_str()) {
                conflict.title = Some(project.title.clone());
            }
        }
    }
}

pub(crate) async fn resolve(
    client: &ModrinthClient,
    roots: &[ModrinthVersion],
    target: &ModrinthResolveTarget,
    installed: &ModrinthInstalled,
) -> Result<ModrinthDependencyResolution> {
    Resolver {
        client,
        target,
        installed,
        visited: HashSet::new(),
        result: ModrinthDependencyResolution::default(),
    }
    .run(roots)
    .await
}
