use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

const MANIFEST_FILE: &str = "bundle.toml";
const BUNDLED_DIRECTORY: &str = "bundled";
const CURRENT_MANIFEST: &str = "current.toml";
const BUNDLE_FORMAT_VERSION: u32 = 1;
const CONTENT_ID_LENGTH: usize = 64;
const REQUIRED_MODEL_FILES: &[&str] = &[
    "completion/en/vocab.txt",
    "completion/en/unigrams.bin",
    "completion/en/bigrams.bin",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    format_version: u32,
    id: String,
    files: Vec<String>,
}

impl Bundle {
    fn parse(text: &str) -> Result<Self> {
        let bundle: Self = toml::from_str(text).context("read bundled asset manifest")?;
        if bundle.format_version != BUNDLE_FORMAT_VERSION
            || bundle.id.len() != CONTENT_ID_LENGTH
            || !bundle
                .id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            bail!("unsupported bundled asset manifest");
        }

        for file in &bundle.files {
            let components: Vec<_> = file.split('/').collect();
            let safe = components.iter().all(|part| {
                !part.is_empty()
                    && *part != "."
                    && *part != ".."
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            });
            let theme =
                components.len() == 2 && components[0] == "themes" && file.ends_with(".toml");
            let model = components.len() == 3
                && components[..2] == ["completion", "en"]
                && matches!(
                    components[2],
                    "vocab.txt"
                        | "unigrams.bin"
                        | "bigrams.bin"
                        | "trigrams.bin"
                        | "manifest.toml"
                        | "unigrams.tsv"
                        | "README.md"
                );
            if !safe || !(theme || model) {
                bail!("invalid bundled asset path: {file}");
            }
        }

        if REQUIRED_MODEL_FILES
            .iter()
            .any(|file| !bundle.files.iter().any(|f| f == file))
            || !bundle.files.iter().any(|file| file.starts_with("themes/"))
        {
            bail!("bundled themes or completion tables are missing");
        }
        Ok(bundle)
    }

    fn directory(&self, config_path: &Path) -> PathBuf {
        config_path.with_file_name(BUNDLED_DIRECTORY).join(&self.id)
    }
}

pub(super) fn provision(config_path: &Path, resources: &Path) -> Result<()> {
    let manifest_path = resources.join(MANIFEST_FILE);
    if !manifest_path.exists() {
        return Ok(());
    }

    let text = std::fs::read_to_string(&manifest_path)?;
    let bundle = Bundle::parse(&text)?;
    let destination = bundle.directory(config_path);
    let root = config_path.with_file_name(BUNDLED_DIRECTORY);
    if !destination.exists() {
        let assets = bundle
            .files
            .iter()
            .map(|file| {
                let bytes = std::fs::read(resources.join(file))
                    .with_context(|| format!("read bundled asset {file}"))?;
                if bytes.is_empty() {
                    bail!("empty bundled asset: {file}");
                }
                Ok((file, bytes))
            })
            .collect::<Result<Vec<_>>>()?;

        std::fs::create_dir_all(&root)?;
        let staging = root.join(format!(".{}.{}.tmp", bundle.id, std::process::id()));
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        std::fs::create_dir(&staging)?;
        let result = install_assets(&staging, &destination, &assets);
        if staging.exists() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        result?;
    }

    for file in &bundle.files {
        if !destination.join(file).is_file() {
            bail!("bundled asset is missing: {file}; reinstall KOSK");
        }
    }

    let current = root.join(CURRENT_MANIFEST);
    if std::fs::read_to_string(&current).ok().as_deref() == Some(&text) {
        return Ok(());
    }
    let temporary = root.join(format!("current.{}.tmp", std::process::id()));
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, &current).context("activate bundled assets")?;
    Ok(())
}

fn install_assets(staging: &Path, destination: &Path, assets: &[(&String, Vec<u8>)]) -> Result<()> {
    for (file, bytes) in assets {
        let path = staging.join(file);
        std::fs::create_dir_all(path.parent().context("bundled asset has no directory")?)?;
        std::fs::write(path, bytes)?;
    }

    match std::fs::rename(staging, destination) {
        Ok(()) => Ok(()),
        Err(_) if destination.is_dir() => Ok(()),
        Err(error) => Err(error).context("install bundled assets"),
    }
}

pub(super) fn apply_defaults(config_path: &Path, defaults: &mut toml::Value) -> Result<()> {
    let manifest = config_path
        .with_file_name(BUNDLED_DIRECTORY)
        .join(CURRENT_MANIFEST);
    if !manifest.exists() {
        return Ok(());
    }

    let bundle = Bundle::parse(&std::fs::read_to_string(manifest)?)?;
    let relative = format!("{BUNDLED_DIRECTORY}/{}", bundle.id);
    defaults["themes"] = toml::Value::Array(vec![
        toml::Value::String(format!("{relative}/themes/*.toml")),
        toml::Value::String("themes/*.toml".into()),
    ]);
    defaults["completion"]["ngram"]["model_dir"] =
        toml::Value::String(format!("{relative}/completion/en"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            Self(std::env::temp_dir().join(format!("kosk-bundle-{}-{unique}", std::process::id())))
        }

        fn release(&self, id: char) -> PathBuf {
            let resources = self.0.join(format!("release-{id}"));
            let files = [
                "themes/amber.toml",
                "completion/en/vocab.txt",
                "completion/en/unigrams.bin",
                "completion/en/bigrams.bin",
            ];
            for file in files {
                let path = resources.join(file);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                let bytes: &[u8] = match file {
                    "themes/amber.toml" => b"name = 'Amber'\n",
                    "completion/en/vocab.txt" => b"hello\n",
                    "completion/en/unigrams.bin" => &[1, 0, 0, 0],
                    _ => &[0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
                };
                std::fs::write(path, bytes).unwrap();
            }
            let manifest = format!(
                "format_version = 1\nid = {:?}\nfiles = {:?}\n",
                id.to_string().repeat(CONTENT_ID_LENGTH),
                files
            );
            std::fs::write(resources.join(MANIFEST_FILE), manifest).unwrap();
            resources
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn fresh_setup_and_upgrade_preserve_sparse_user_settings_and_custom_assets() {
        let fixture = Fixture::new();
        let config = fixture.0.join("user/config.toml");
        std::fs::create_dir_all(config.parent().unwrap().join("themes")).unwrap();
        let settings = "config_version = 3\nrenderer = 'wgpu'\nunknown_future_key = true\n";
        std::fs::write(&config, settings).unwrap();
        let custom = config.with_file_name("themes").join("custom.toml");
        std::fs::write(&custom, "name = 'Custom'\n").unwrap();
        let first = fixture.release('a');
        provision(&config, &first).unwrap();
        provision(&config, &first).unwrap();

        let second = fixture.release('b');
        provision(&config, &second).unwrap();
        assert_eq!(std::fs::read_to_string(&config).unwrap(), settings);
        assert_eq!(
            std::fs::read_to_string(&custom).unwrap(),
            "name = 'Custom'\n"
        );
        let (cfg, _) =
            super::super::read_merged_config(&config, super::super::ConfigSource::User).unwrap();
        assert_eq!(cfg.renderer, super::super::Renderer::Wgpu);
        assert_eq!(cfg.theme_names(), vec!["default", "Amber", "Custom"]);
        let model = config
            .parent()
            .unwrap()
            .join(&cfg.completion.ngram.model_dir);
        let cache = std::sync::Arc::new(std::sync::Mutex::new(crate::completion::UserCache::load(
            &cfg.completion.user_cache,
            config.with_file_name("cache.bin"),
        )));
        let loaded =
            crate::completion::backend_from_config(&cfg.completion, config.parent(), cache)
                .unwrap();
        assert_eq!(loaded.next_word, crate::completion::NextWordTables::Ready);
        assert!(model.join("bigrams.bin").is_file());
        super::super::write_user_overlay(&config, &cfg, None).unwrap();
        let saved = std::fs::read_to_string(&config).unwrap();
        assert!(!saved.contains("model_dir"));
        assert!(!saved.contains("themes"));
        assert!(saved.contains("unknown_future_key"));
        let (explicit, _) =
            super::super::read_merged_config(&config, super::super::ConfigSource::Explicit)
                .unwrap();
        assert!(explicit.themes.is_empty());
        assert_eq!(
            explicit.completion.ngram.model_dir,
            super::super::Config::default().completion.ngram.model_dir
        );
    }

    #[test]
    fn incomplete_release_keeps_previous_bundle_active() {
        let fixture = Fixture::new();
        let config = fixture.0.join("user/config.toml");
        let first = fixture.release('a');
        provision(&config, &first).unwrap();
        let marker = config
            .with_file_name(BUNDLED_DIRECTORY)
            .join(CURRENT_MANIFEST);
        let before = std::fs::read(&marker).unwrap();
        let broken = fixture.release('b');
        std::fs::remove_file(broken.join("completion/en/bigrams.bin")).unwrap();
        assert!(provision(&config, &broken).is_err());
        assert_eq!(std::fs::read(marker).unwrap(), before);
        assert!(!config
            .with_file_name(BUNDLED_DIRECTORY)
            .join("b".repeat(CONTENT_ID_LENGTH))
            .exists());
    }

    #[test]
    fn custom_model_path_survives_provisioning_and_saving() {
        let fixture = Fixture::new();
        let config = fixture.0.join("user/config.toml");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(
            &config,
            "config_version = 3\n[completion.ngram]\nmodel_dir = 'my-model'\n",
        )
        .unwrap();
        provision(&config, &fixture.release('a')).unwrap();
        let (cfg, _) =
            super::super::read_merged_config(&config, super::super::ConfigSource::User).unwrap();
        assert_eq!(cfg.completion.ngram.model_dir, Path::new("my-model"));
        super::super::write_user_overlay(&config, &cfg, None).unwrap();
        assert!(std::fs::read_to_string(config)
            .unwrap()
            .contains("my-model"));
    }

    #[test]
    fn blocked_destination_preserves_existing_settings() {
        let fixture = Fixture::new();
        let config = fixture.0.join("user/config.toml");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(&config, "config_version = 3\n").unwrap();
        std::fs::write(config.with_file_name(BUNDLED_DIRECTORY), "blocked").unwrap();
        assert!(provision(&config, &fixture.release('a')).is_err());
        assert_eq!(
            std::fs::read_to_string(config).unwrap(),
            "config_version = 3\n"
        );
    }

    #[test]
    fn development_launch_needs_no_bundle_and_invalid_paths_are_rejected() {
        let fixture = Fixture::new();
        let config = fixture.0.join("user/config.toml");
        provision(&config, &fixture.0.join("absent")).unwrap();
        assert!(!fixture.0.exists());
        for path in [
            "../outside",
            "C:/outside",
            "themes/../outside.toml",
            "themes/bad\\name.toml",
        ] {
            let text = format!(
                "format_version = 1\nid = {:?}\nfiles = [{path:?}]\n",
                "a".repeat(CONTENT_ID_LENGTH)
            );
            assert!(Bundle::parse(&text).is_err(), "{path}");
        }
    }
}
