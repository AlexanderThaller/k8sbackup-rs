use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use kube::{
    Api, Client, ResourceExt,
    api::{DynamicObject, ListParams},
    discovery::{ApiCapabilities, ApiResource, Discovery, Scope, verbs},
};
use serde_json::Value;

const GLOBAL_NAMESPACE: &str = "_global";

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Dump Kubernetes objects to restore-friendly YAML files"
)]
struct Args {
    /// Output directory for the backup
    #[arg(short, long, default_value = "backup")]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    fs::create_dir_all(&args.output)
        .with_context(|| format!("creating output directory {}", args.output.display()))?;

    let client = Client::try_default()
        .await
        .context("creating Kubernetes client from local kubeconfig or in-cluster config")?;

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

            match dump_resource(&client, &resource, &capabilities, &args.output).await {
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

    println!("wrote {written} object(s) to {}", args.output.display());

    if !failures.is_empty() {
        eprintln!("failed to dump {} resource type(s):", failures.len());
        for failure in failures {
            eprintln!("  - {failure}");
        }
        bail!("backup completed with errors");
    }

    Ok(())
}

fn should_skip_resource(resource: &ApiResource, capabilities: &ApiCapabilities) -> bool {
    resource.plural.contains('/')
        || !capabilities.supports_operation(verbs::LIST)
        || !capabilities.supports_operation(verbs::GET)
}

async fn dump_resource(
    client: &Client,
    resource: &ApiResource,
    capabilities: &ApiCapabilities,
    output: &Path,
) -> Result<usize> {
    let api: Api<DynamicObject> = Api::all_with(client.clone(), resource);
    let objects = api
        .list(&ListParams::default())
        .await
        .with_context(|| format!("listing {}", resource.plural))?;

    let mut count = 0usize;
    for object in objects {
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

        write_object(&path, object).with_context(|| format!("writing {}", path.display()))?;
        count += 1;
    }

    Ok(count)
}

fn write_object(path: &Path, object: DynamicObject) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)?;

    let mut value = serde_json::to_value(object)?;
    clean_for_restore(&mut value);

    let yaml = serde_yaml::to_string(&value)?;
    fs::write(path, yaml)?;

    Ok(())
}

fn clean_for_restore(value: &mut Value) {
    let Value::Object(object) = value else {
        return;
    };

    object.remove("status");

    let Some(Value::Object(metadata)) = object.get_mut("metadata") else {
        return;
    };

    for key in [
        "creationTimestamp",
        "deletionGracePeriodSeconds",
        "deletionTimestamp",
        "generation",
        "managedFields",
        "ownerReferences",
        "resourceVersion",
        "selfLink",
        "uid",
    ] {
        metadata.remove(key);
    }
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
