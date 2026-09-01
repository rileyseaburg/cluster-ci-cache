//! Deterministic cache-key generation helpers.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::Context;

const API_UI_ASSETS_INCLUDES: &[&str] = &[
    ".npmrc",
    ".spotless-cache",
    "api",
    "bindings",
    "components",
    "config/database/migrations",
    "facebook-business-api-sdk",
    "package.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "postcss.config.mjs",
    "rspack.api.config.js",
    "rspack.config.js",
    "rustyroad.dev.toml",
    "rustyroad.prod.toml",
    "rustyroad.test.toml",
    "scripts/add-js-extensions.js",
    "scripts/export-tailwind-db-content.mjs",
    "scripts/derive-public-types-from-rustyroad.js",
    "scripts/generate-db-types.js",
    "scripts/generate-api-client-atomic.mjs",
    "spotless-web/ui",
    "src",
    "static",
    "tailwind.config.js",
    "tsconfig.json",
    "scripts/ci/api-assets-cache-key.sh",
];

const API_UI_ASSETS_EXACT_EXCLUDES: &[&str] = &[
    "api/dist",
    "api/public",
    "api/src/openapi.json",
    "api/create-indexes.sh",
    "api/db-schema.txt",
    "api/output.txt",
    "api/rrweb.ndjson",
    "api/refactor.md",
];

const API_UI_ASSETS_PREFIX_EXCLUDES: &[&str] = &[
    "api/dist/",
    "api/public/",
    "api/docs/",
    "api/helm/",
    "api/k8s/",
    "api/.vscode/",
    "api/debug-",
];

/// Options for a git tree based cache key.
pub struct GitTreeKeyOptions {
    pub repo_root: PathBuf,
    pub revision: String,
    pub prefix: String,
    pub includes: Vec<String>,
    pub exclude: Vec<String>,
    pub exclude_prefix: Vec<String>,
    pub include_platform: bool,
}

pub fn api_ui_assets_key(repo_root: &Path, revision: &str) -> anyhow::Result<String> {
    git_tree_key(GitTreeKeyOptions {
        repo_root: repo_root.to_path_buf(),
        revision: revision.to_string(),
        prefix: "api-ui-assets-v5".to_string(),
        includes: API_UI_ASSETS_INCLUDES
            .iter()
            .map(|s| s.to_string())
            .collect(),
        exclude: API_UI_ASSETS_EXACT_EXCLUDES
            .iter()
            .map(|s| s.to_string())
            .collect(),
        exclude_prefix: API_UI_ASSETS_PREFIX_EXCLUDES
            .iter()
            .map(|s| s.to_string())
            .collect(),
        include_platform: true,
    })
}

pub fn git_tree_key(options: GitTreeKeyOptions) -> anyhow::Result<String> {
    let repo_root = canonical_repo_root(&options.repo_root)?;
    let tree = git_ls_tree(&repo_root, &options.revision, &options.includes)?;
    let filtered = filter_tree(&tree, &options.exclude, &options.exclude_prefix);
    let digest = git_hash_object(&repo_root, filtered.as_bytes())?;

    if options.include_platform {
        Ok(format!(
            "{}-{}-{}-{}",
            options.prefix,
            std::env::consts::OS,
            portable_arch(),
            digest
        ))
    } else {
        Ok(format!("{}-{}", options.prefix, digest))
    }
}

fn canonical_repo_root(path: &Path) -> anyhow::Result<PathBuf> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .with_context(|| format!("failed to run git in {}", path.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to find git repo root: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(PathBuf::from(String::from_utf8(output.stdout)?.trim()))
}

fn git_ls_tree(repo_root: &Path, revision: &str, includes: &[String]) -> anyhow::Result<String> {
    if includes.is_empty() {
        anyhow::bail!("at least one --include path is required");
    }

    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(repo_root)
        .args(["ls-tree", "-r", "--full-tree", revision, "--"])
        .args(includes);
    let output = cmd.output().context("failed to run git ls-tree")?;
    if !output.status.success() {
        anyhow::bail!(
            "git ls-tree failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn git_hash_object(repo_root: &Path, bytes: &[u8]) -> anyhow::Result<String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["hash-object", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to run git hash-object")?;

    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .context("failed to open git hash-object stdin")?
            .write_all(bytes)?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        anyhow::bail!(
            "git hash-object failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn filter_tree(tree: &str, exact_excludes: &[String], prefix_excludes: &[String]) -> String {
    tree.lines()
        .filter(|line| {
            let Some((_, path)) = line.split_once('\t') else {
                return true;
            };
            !is_api_ui_assets_generated_or_non_input(path)
                && !exact_excludes.iter().any(|exclude| path == exclude)
                && !prefix_excludes
                    .iter()
                    .any(|exclude_prefix| path.starts_with(exclude_prefix))
        })
        .map(|line| format!("{line}\n"))
        .collect()
}

fn is_api_ui_assets_generated_or_non_input(path: &str) -> bool {
    path.starts_with("api/")
        && (path.ends_with(".md") || path.ends_with(".test.ts") || path.ends_with(".spec.ts"))
}

fn portable_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" | "amd64" => "X64",
        "aarch64" | "arm64" => "ARM64",
        arch => arch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_exact_prefix_and_non_input_api_paths() {
        let tree = concat!(
            "100644 blob aaa\tapi/src/index.ts\n",
            "100644 blob bbb\tapi/dist/index.js\n",
            "100644 blob ccc\tapi/docs/readme.md\n",
            "100644 blob ddd\tapi/foo.test.ts\n",
            "100644 blob eee\tpackage.json\n",
        );
        let filtered = filter_tree(
            tree,
            &["package.json".to_string()],
            &["api/dist/".to_string(), "api/docs/".to_string()],
        );
        assert_eq!(filtered, "100644 blob aaa\tapi/src/index.ts\n");
    }

    #[test]
    fn maps_portable_arch() {
        assert!(!portable_arch().is_empty());
    }
}
