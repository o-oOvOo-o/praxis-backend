mod agents_md;
mod claude;
mod ledger;
mod skills;
mod target_config;

use ledger::ExternalAgentMigrationLedger;
use ledger::emit_import_metric;
use std::io;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAgentMigrationDetectOptions {
    pub include_home: bool,
    pub cwds: Option<Vec<PathBuf>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalAgentMigrationItemType {
    Config,
    Skills,
    AgentsMd,
    McpServerConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAgentMigrationItem {
    pub item_type: ExternalAgentMigrationItemType,
    pub description: String,
    pub cwd: Option<PathBuf>,
}

#[derive(Clone)]
pub struct ExternalAgentMigrationService {
    praxis_home: PathBuf,
    claude_home: PathBuf,
}

#[derive(Debug, Clone)]
struct ExternalAgentMigrationScope {
    cwd: Option<PathBuf>,
    kind: ExternalAgentMigrationScopeKind,
    praxis_home: PathBuf,
    claude_home: PathBuf,
}

#[derive(Debug, Clone)]
enum ExternalAgentMigrationScopeKind {
    Home,
    Repo { root: PathBuf },
}

impl ExternalAgentMigrationScope {
    fn home(praxis_home: PathBuf, claude_home: PathBuf) -> Self {
        Self {
            cwd: None,
            kind: ExternalAgentMigrationScopeKind::Home,
            praxis_home,
            claude_home,
        }
    }

    fn repo(repo_root: PathBuf, praxis_home: PathBuf, claude_home: PathBuf) -> Self {
        Self {
            cwd: Some(repo_root.clone()),
            kind: ExternalAgentMigrationScopeKind::Repo { root: repo_root },
            praxis_home,
            claude_home,
        }
    }

    fn source_settings(&self) -> PathBuf {
        match &self.kind {
            ExternalAgentMigrationScopeKind::Home => claude::home_settings(&self.claude_home),
            ExternalAgentMigrationScopeKind::Repo { root } => claude::repo_settings(root),
        }
    }

    fn target_config(&self) -> PathBuf {
        match &self.kind {
            ExternalAgentMigrationScopeKind::Home => target_config::home_path(&self.praxis_home),
            ExternalAgentMigrationScopeKind::Repo { root } => target_config::repo_path(root),
        }
    }

    fn source_skills(&self) -> PathBuf {
        match &self.kind {
            ExternalAgentMigrationScopeKind::Home => claude::home_skills(&self.claude_home),
            ExternalAgentMigrationScopeKind::Repo { root } => claude::repo_skills(root),
        }
    }

    fn target_skills(&self) -> PathBuf {
        match &self.kind {
            ExternalAgentMigrationScopeKind::Home => skills::home_target(&self.praxis_home),
            ExternalAgentMigrationScopeKind::Repo { root } => skills::repo_target(root),
        }
    }

    fn source_agents_md(&self) -> io::Result<Option<PathBuf>> {
        match &self.kind {
            ExternalAgentMigrationScopeKind::Home => agents_md::home_source(&self.claude_home),
            ExternalAgentMigrationScopeKind::Repo { root } => agents_md::repo_source(root),
        }
    }

    fn target_agents_md(&self) -> PathBuf {
        match &self.kind {
            ExternalAgentMigrationScopeKind::Home => agents_md::home_target(&self.praxis_home),
            ExternalAgentMigrationScopeKind::Repo { root } => agents_md::repo_target(root),
        }
    }
}

impl ExternalAgentMigrationService {
    pub fn new(praxis_home: PathBuf) -> Self {
        let claude_home = claude::default_home();
        Self {
            praxis_home,
            claude_home,
        }
    }

    #[cfg(test)]
    fn new_for_test(praxis_home: PathBuf, claude_home: PathBuf) -> Self {
        Self {
            praxis_home,
            claude_home,
        }
    }

    pub fn detect(
        &self,
        params: ExternalAgentMigrationDetectOptions,
    ) -> io::Result<Vec<ExternalAgentMigrationItem>> {
        let mut ledger = ExternalAgentMigrationLedger::default();
        if params.include_home {
            let scope = self.home_scope();
            self.detect_migrations(&scope, &mut ledger)?;
        }

        for cwd in params.cwds.as_deref().unwrap_or(&[]) {
            let Some(scope) = self.detect_scope_for_cwd(cwd)? else {
                continue;
            };
            self.detect_migrations(&scope, &mut ledger)?;
        }

        Ok(ledger.into_items())
    }

    pub fn import(&self, migration_items: Vec<ExternalAgentMigrationItem>) -> io::Result<()> {
        for migration_item in migration_items {
            self.import_item(migration_item)?;
        }

        Ok(())
    }

    fn import_item(&self, migration_item: ExternalAgentMigrationItem) -> io::Result<()> {
        if migration_item.item_type == ExternalAgentMigrationItemType::McpServerConfig {
            return Ok(());
        }

        let Some(scope) = self.migration_scope(migration_item.cwd.as_deref())? else {
            return Ok(());
        };

        let skills_count = match migration_item.item_type {
            ExternalAgentMigrationItemType::Config => {
                self.import_config_from_scope(&scope)?;
                None
            }
            ExternalAgentMigrationItemType::Skills => Some(self.import_skills_from_scope(&scope)?),
            ExternalAgentMigrationItemType::AgentsMd => {
                self.import_agents_md_from_scope(&scope)?;
                None
            }
            ExternalAgentMigrationItemType::McpServerConfig => unreachable!(),
        };

        emit_import_metric(migration_item.item_type, skills_count);
        Ok(())
    }

    fn home_scope(&self) -> ExternalAgentMigrationScope {
        ExternalAgentMigrationScope::home(self.praxis_home.clone(), self.claude_home.clone())
    }

    fn repo_scope(&self, repo_root: PathBuf) -> ExternalAgentMigrationScope {
        ExternalAgentMigrationScope::repo(
            repo_root,
            self.praxis_home.clone(),
            self.claude_home.clone(),
        )
    }

    fn detect_scope_for_cwd(&self, cwd: &Path) -> io::Result<Option<ExternalAgentMigrationScope>> {
        Ok(find_repo_root(Some(cwd))?.map(|repo_root| self.repo_scope(repo_root)))
    }

    fn migration_scope(
        &self,
        cwd: Option<&Path>,
    ) -> io::Result<Option<ExternalAgentMigrationScope>> {
        if let Some(repo_root) = find_repo_root(cwd)? {
            return Ok(Some(self.repo_scope(repo_root)));
        }
        if cwd.is_some_and(|cwd| !cwd.as_os_str().is_empty()) {
            return Ok(None);
        }
        Ok(Some(self.home_scope()))
    }

    fn detect_migrations(
        &self,
        scope: &ExternalAgentMigrationScope,
        ledger: &mut ExternalAgentMigrationLedger,
    ) -> io::Result<()> {
        let source_settings = scope.source_settings();
        let target_config_path = scope.target_config();
        if let Some(migrated) = claude::load_migrated_config(&source_settings)?
            && target_config::needs_values(&target_config_path, &migrated)?
        {
            ledger.push_detected(
                scope.cwd.clone(),
                ExternalAgentMigrationItemType::Config,
                format!(
                    "Migrate {} into {}",
                    source_settings.display(),
                    target_config_path.display()
                ),
                /*skills_count*/ None,
            );
        }

        let source_skills = scope.source_skills();
        let target_skills = scope.target_skills();
        let skills_count = skills::count_missing(&source_skills, &target_skills)?;
        if skills_count > 0 {
            ledger.push_detected(
                scope.cwd.clone(),
                ExternalAgentMigrationItemType::Skills,
                format!(
                    "Copy skill folders from {} to {}",
                    source_skills.display(),
                    target_skills.display()
                ),
                Some(skills_count),
            );
        }

        let target_agents_md = scope.target_agents_md();
        if let Some(source_agents_md) = scope.source_agents_md()?
            && agents_md::target_needs_import(&target_agents_md)?
        {
            ledger.push_detected(
                scope.cwd.clone(),
                ExternalAgentMigrationItemType::AgentsMd,
                format!(
                    "Import {} to {}",
                    source_agents_md.display(),
                    target_agents_md.display()
                ),
                /*skills_count*/ None,
            );
        }

        Ok(())
    }

    fn import_config_from_scope(&self, scope: &ExternalAgentMigrationScope) -> io::Result<()> {
        let source_settings = scope.source_settings();
        let Some(migrated) = claude::load_migrated_config(&source_settings)? else {
            return Ok(());
        };

        target_config::merge_or_create(&scope.target_config(), &migrated)
    }

    #[cfg(test)]
    fn import_skills(&self, cwd: Option<&Path>) -> io::Result<usize> {
        let Some(scope) = self.migration_scope(cwd)? else {
            return Ok(0);
        };
        self.import_skills_from_scope(&scope)
    }

    fn import_skills_from_scope(&self, scope: &ExternalAgentMigrationScope) -> io::Result<usize> {
        let source_skills = scope.source_skills();
        let target_skills = scope.target_skills();
        skills::import_missing(&source_skills, &target_skills)
    }

    fn import_agents_md_from_scope(&self, scope: &ExternalAgentMigrationScope) -> io::Result<()> {
        let Some(source_agents_md) = scope.source_agents_md()? else {
            return Ok(());
        };
        agents_md::import(&source_agents_md, &scope.target_agents_md())
    }
}

fn find_repo_root(cwd: Option<&Path>) -> io::Result<Option<PathBuf>> {
    let Some(cwd) = cwd.filter(|cwd| !cwd.as_os_str().is_empty()) else {
        return Ok(None);
    };

    let mut current = if cwd.is_absolute() {
        cwd.to_path_buf()
    } else {
        std::env::current_dir()?.join(cwd)
    };

    if !current.exists() {
        return Ok(None);
    }

    if current.is_file() {
        let Some(parent) = current.parent() else {
            return Ok(None);
        };
        current = parent.to_path_buf();
    }

    let fallback = current.clone();
    loop {
        let git_path = current.join(".git");
        if git_path.is_dir() || git_path.is_file() {
            return Ok(Some(current));
        }
        if !current.pop() {
            break;
        }
    }

    Ok(Some(fallback))
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
