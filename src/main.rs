use std::{
    fs,
    io::BufWriter,
    path::{
        Path,
        PathBuf,
    },
};

use anyhow::{
    Context,
    Result,
    anyhow,
    bail,
};
use clap::{
    Parser,
    ValueEnum,
};
use kube::{
    Api,
    Client,
    ResourceExt,
    api::{
        DynamicObject,
        ListParams,
    },
    core::TypeMeta,
    discovery::{
        ApiCapabilities,
        ApiResource,
        Discovery,
        Scope,
        verbs,
    },
};
const GLOBAL_NAMESPACE: &str = "_global";
const KUBERNETES_LIST_PAGE_SIZE: u32 = 100;
const RESTIC_COMPRESSION_LEVEL: i32 = 1;
const RESTIC_SNAPSHOT_PATH: &str = "k8sbackup";
const RESTIC_TAG: &str = "k8sbackup";

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Dump Kubernetes objects to restore-friendly YAML files"
)]
struct Args {
    /// Backup destination type.
    #[arg(long = "backup-type", value_enum, default_value = "folder")]
    backup_type: BackupType,

    /// Output directory for `--backup-type folder`.
    #[arg(short, long, default_value = "backup")]
    output: PathBuf,

    /// Restic repository destination for `--backup-type restic`.
    #[arg(
        long = "restic-repository",
        env = "K8SBACKUP_RESTIC_REPOSITORY",
        value_name = "REPOSITORY",
        required_if_eq("backup_type", "restic")
    )]
    restic_repository: Option<String>,

    /// Restic repository password.
    ///
    /// Falls back to `K8SBACKUP_RESTIC_PASSWORD`, then `RESTIC_PASSWORD`.
    #[arg(
        long = "restic-password",
        env = "K8SBACKUP_RESTIC_PASSWORD",
        hide_env_values = true
    )]
    restic_password: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BackupType {
    Folder,
    Restic,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let args = Args::parse();

    let client = Client::try_default()
        .await
        .context("creating Kubernetes client from local kubeconfig or in-cluster config")?;

    match args.backup_type {
        BackupType::Folder => {
            let written = dump_cluster(&client, &args.output).await?;
            println!("wrote {written} object(s) to {}", args.output.display());
        }
        BackupType::Restic => {
            let repository = args.restic_repository.as_deref().ok_or_else(|| {
                anyhow!("--restic-repository is required for --backup-type restic")
            })?;
            let password = restic_password(args.restic_password)?;
            let written = write_restic_backup(&client, repository, password).await?;
            println!(
                "wrote {written} object(s) to restic repository {}",
                censor_repository_password(repository)
            );
        }
    }

    Ok(())
}

async fn dump_cluster(client: &Client, output: &Path) -> Result<usize> {
    fs::create_dir_all(output)
        .with_context(|| format!("creating output directory {}", output.display()))?;

    let discovery = Discovery::new(client.clone())
        .run()
        .await
        .context("discovering Kubernetes API resources")?;

    let mut failures = Vec::new();
    let mut written = 0usize;

    for group in discovery.groups() {
        for (resource, capabilities) in group.recommended_resources() {
            if should_skip_resource(&resource, &capabilities) {
                continue;
            }

            match dump_resource(client, &resource, &capabilities, output).await {
                Ok(count) => written += count,
                Err(err) => {
                    failures.push(format!(
                        "{}/{}: {err:#}",
                        resource.api_version, resource.kind
                    ));
                }
            }
        }
    }

    if !failures.is_empty() {
        eprintln!("failed to dump {} resource type(s):", failures.len());
        for failure in failures {
            eprintln!("  - {failure}");
        }
        bail!("backup completed with errors");
    }

    Ok(written)
}

fn should_skip_resource(resource: &ApiResource, capabilities: &ApiCapabilities) -> bool {
    resource.plural.contains('/')
        || !capabilities.supports_operation(verbs::LIST)
        || !capabilities.supports_operation(verbs::GET)
}

fn restic_password(password: Option<String>) -> Result<String> {
    password
        .or_else(|| std::env::var("K8SBACKUP_RESTIC_PASSWORD").ok())
        .or_else(|| std::env::var("RESTIC_PASSWORD").ok())
        .ok_or_else(|| {
            anyhow!(
                "--restic-password, K8SBACKUP_RESTIC_PASSWORD, or RESTIC_PASSWORD is required for \
                 --backup-type restic"
            )
        })
}

async fn write_restic_backup(client: &Client, repository: &str, password: String) -> Result<usize> {
    let staging = std::env::temp_dir().join(format!("k8sbackup-{}", uuid::Uuid::now_v7()));

    let result = async {
        let written = dump_cluster(client, &staging).await?;
        let source = staging.clone();
        let repository = repository.to_string();

        tokio::task::spawn_blocking(move || run_rustic_backup(&source, &repository, password))
            .await
            .context("restic backup task failed")??;

        Ok(written)
    }
    .await;

    if let Err(err) = fs::remove_dir_all(&staging) {
        eprintln!(
            "warning: failed to remove temporary backup staging directory {}: {err}",
            staging.display()
        );
    }

    result
}

fn run_rustic_backup(source: &Path, repository: &str, password: String) -> Result<()> {
    use rustic_backend::BackendOptions;
    use rustic_core::{
        BackupOptions,
        CheckOptions,
        ConfigOptions,
        Credentials,
        KeyOptions,
        PathList,
        Repository,
        RepositoryOptions,
        SnapshotOptions,
    };

    let credentials = Credentials::Password(password);
    let repo_opts = RepositoryOptions::default();
    let backends = BackendOptions::default()
        .repository(repository)
        .to_backends()?;
    let mut config_opts = ConfigOptions::default();
    config_opts.set_compression = Some(RESTIC_COMPRESSION_LEVEL);

    let repo = Repository::new(&repo_opts, &backends)?;
    let mut repo = match repo.open(&credentials) {
        Ok(repo) => repo,
        Err(_) => Repository::new(&repo_opts, &backends)?.init(
            &credentials,
            &KeyOptions::default(),
            &config_opts,
        )?,
    };
    if repo.apply_config(&config_opts)? {
        repo = Repository::new(&repo_opts, &backends)?.open(&credentials)?;
    }
    let repo = repo.to_indexed_ids()?;

    let mut backup_opts = BackupOptions::default();
    backup_opts.as_path = Some(PathBuf::from(RESTIC_SNAPSHOT_PATH));
    let source = source
        .to_str()
        .ok_or_else(|| anyhow!("backup staging path is not valid UTF-8"))?;
    let source = PathList::from_string(source)?.sanitize()?;
    let snapshot = SnapshotOptions::default()
        .add_tags(RESTIC_TAG)?
        .to_snapshot()?;
    let snapshot = repo.backup(&backup_opts, &source, snapshot)?;
    println!("created restic snapshot {}", snapshot.id);

    let check_opts = CheckOptions::default();
    let results = repo.check(check_opts)?;
    results.is_ok()?;
    println!("checked repository");

    Ok(())
}

async fn dump_resource(
    client: &Client,
    resource: &ApiResource,
    capabilities: &ApiCapabilities,
    output: &Path,
) -> Result<usize> {
    let api: Api<DynamicObject> = Api::all_with(client.clone(), resource);
    let mut list_params = ListParams::default().limit(KUBERNETES_LIST_PAGE_SIZE);
    let mut count = 0usize;

    loop {
        let mut objects = api
            .list(&list_params)
            .await
            .with_context(|| format!("listing {}", resource.plural))?;
        let continue_token = objects
            .metadata
            .continue_
            .take()
            .filter(|token| !token.is_empty());

        for mut object in objects {
            ensure_type_meta(&mut object, resource);

            let namespace = match capabilities.scope {
                Scope::Cluster => GLOBAL_NAMESPACE.to_string(),
                Scope::Namespaced => object.namespace().ok_or_else(|| {
                    anyhow!("namespaced object {} had no namespace", object.name_any())
                })?,
            };

            let filename = format!("{}.yaml", safe_path_segment(&object.name_any()));
            let resource_dir = format!(
                "{}-{}",
                safe_path_segment(&resource.kind),
                safe_path_segment(&resource.api_version)
            );
            let path = output
                .join(safe_path_segment(&namespace))
                .join(resource_dir)
                .join(filename);

            write_object(&path, &object).with_context(|| format!("writing {}", path.display()))?;
            count += 1;
        }

        match continue_token {
            Some(token) => {
                list_params = ListParams::default()
                    .limit(KUBERNETES_LIST_PAGE_SIZE)
                    .continue_token(&token);
            }
            None => break,
        }
    }

    Ok(count)
}

fn write_object(path: &Path, object: &DynamicObject) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)?;

    let file = fs::File::create(path)?;
    serde_yaml::to_writer(BufWriter::new(file), object)?;

    Ok(())
}

fn ensure_type_meta(object: &mut DynamicObject, resource: &ApiResource) {
    object.types.get_or_insert_with(|| TypeMeta {
        api_version: resource.api_version.clone(),
        kind: resource.kind.clone(),
    });
}

fn safe_path_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => ch,
            _ => '_',
        })
        .collect()
}

fn censor_repository_password(repository: &str) -> String {
    let Some(authority_start) = repository.find("://").map(|index| index + 3) else {
        return repository.to_string();
    };

    let authority = &repository[authority_start..];
    let authority_end = authority
        .find(['/', '?', '#'])
        .map_or(repository.len(), |index| authority_start + index);
    let authority = &repository[authority_start..authority_end];

    let Some(at_index) = authority.rfind('@') else {
        return repository.to_string();
    };
    let userinfo = &authority[..at_index];
    let Some(password_start) = userinfo.find(':') else {
        return repository.to_string();
    };

    let password_start = authority_start + password_start + 1;
    let password_end = authority_start + at_index;

    format!(
        "{}***{}",
        &repository[..password_start],
        &repository[password_end..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use kube::core::{
        ObjectMeta,
        TypeMeta,
    };
    use serde_json::{
        Value,
        json,
    };

    #[test]
    fn ensure_type_meta_adds_missing_api_version_and_kind() {
        let resource = ApiResource {
            group: "apps".to_string(),
            version: "v1".to_string(),
            api_version: "apps/v1".to_string(),
            kind: "Deployment".to_string(),
            plural: "deployments".to_string(),
        };
        let mut object = DynamicObject {
            types: None,
            metadata: ObjectMeta {
                name: Some("example".to_string()),
                ..Default::default()
            },
            data: json!({
                "spec": {
                    "replicas": 1,
                },
            }),
        };

        ensure_type_meta(&mut object, &resource);

        assert_eq!(
            object.types,
            Some(TypeMeta {
                api_version: "apps/v1".to_string(),
                kind: "Deployment".to_string(),
            })
        );
    }

    #[test]
    fn ensure_type_meta_preserves_existing_api_version_and_kind() {
        let resource = ApiResource {
            group: "apps".to_string(),
            version: "v1".to_string(),
            api_version: "apps/v1".to_string(),
            kind: "Deployment".to_string(),
            plural: "deployments".to_string(),
        };
        let mut object = DynamicObject {
            types: Some(TypeMeta {
                api_version: "custom/v1".to_string(),
                kind: "Custom".to_string(),
            }),
            metadata: ObjectMeta {
                name: Some("example".to_string()),
                ..Default::default()
            },
            data: json!({}),
        };

        ensure_type_meta(&mut object, &resource);

        assert_eq!(
            object.types,
            Some(TypeMeta {
                api_version: "custom/v1".to_string(),
                kind: "Custom".to_string(),
            })
        );
    }

    #[test]
    fn censor_repository_password_masks_rest_backend_password() {
        let repository =
            "rest:https://imap-chatbot-k8sbackup:secret@restic.thaller.ws/imap-chatbot-k8sbackup";

        assert_eq!(
            censor_repository_password(repository),
            "rest:https://imap-chatbot-k8sbackup:***@restic.thaller.ws/imap-chatbot-k8sbackup"
        );
    }

    #[test]
    fn censor_repository_password_leaves_repository_without_password_unchanged() {
        let repository = "s3:s3.amazonaws.com/example-bucket";

        assert_eq!(censor_repository_password(repository), repository);
    }

    #[test]
    fn write_object_preserves_complete_object() -> Result<()> {
        let path = std::env::temp_dir().join(format!("k8sbackup-rs-{}.yaml", uuid::Uuid::now_v7()));
        let object = DynamicObject {
            types: Some(TypeMeta {
                api_version: "v1".to_string(),
                kind: "Pod".to_string(),
            }),
            metadata: ObjectMeta {
                name: Some("example".to_string()),
                namespace: Some("default".to_string()),
                resource_version: Some("12345".to_string()),
                uid: Some("abcde".to_string()),
                ..Default::default()
            },
            data: json!({
                "spec": {
                    "containers": [{
                        "name": "example",
                        "image": "alpine",
                    }],
                },
                "status": {
                    "phase": "Running",
                },
            }),
        };

        write_object(&path, &object)?;

        let yaml = fs::read_to_string(&path)?;
        let written: Value = serde_yaml::from_str(&yaml)?;
        let _ = fs::remove_file(&path);

        assert_eq!(
            written.get("apiVersion").and_then(Value::as_str),
            Some("v1")
        );
        assert_eq!(written.get("kind").and_then(Value::as_str), Some("Pod"));
        assert_eq!(
            written
                .pointer("/metadata/resourceVersion")
                .and_then(Value::as_str),
            Some("12345")
        );
        assert_eq!(
            written.pointer("/metadata/uid").and_then(Value::as_str),
            Some("abcde")
        );
        assert_eq!(
            written.pointer("/status/phase").and_then(Value::as_str),
            Some("Running")
        );

        Ok(())
    }
}
