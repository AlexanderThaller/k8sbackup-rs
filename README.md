# k8sbackup-rs

`k8sbackup-rs` backs up Kubernetes API objects as restore-friendly YAML files.
It can write the dump to a local folder or stage the dump and store it in a
restic-compatible repository via `rustic`.

The tool discovers API resources from the current cluster, skips resources that
cannot be listed and fetched, and writes one YAML file per object.

## Usage

Run against the cluster from your current kubeconfig:

```sh
cargo run -- --backup-type folder --output backup
```

Write a backup to a restic repository:

```sh
export K8SBACKUP_RESTIC_REPOSITORY='restic'
export K8SBACKUP_RESTIC_PASSWORD='repository-password'

cargo run -- \
  --backup-type restic
```

The restic repository can also be passed with `--restic-repository`; the
repository password can be passed with `--restic-password`,
`K8SBACKUP_RESTIC_PASSWORD`, or `RESTIC_PASSWORD`.

## Build

Build an optimized local binary:

```sh
cargo build --locked --profile deploy
```

Build the container image:

```sh
podman build -f Containerfile -t k8sbackup-rs:latest .
```

`Containerfile` builds with `cargo build --locked --profile deploy` and copies
the binary into a distroless nonroot runtime image.

## Kubernetes CronJob

The `kubernetes/` directory contains a kustomize deployment with:

- a `k8sbackup` namespace
- a service account
- cluster-wide `get`/`list` RBAC for Kubernetes resources
- a daily CronJob that runs `--backup-type restic`

Kustomize does not natively read secret values from process environment
variables. Create the Secret from your shell environment, then apply the
kustomize resources:

```sh
kubectl apply -f kubernetes/namespace.yaml

kubectl create secret generic k8sbackup-restic \
  --namespace k8sbackup \
  --from-literal=K8SBACKUP_RESTIC_REPOSITORY="$K8SBACKUP_RESTIC_REPOSITORY" \
  --from-literal=K8SBACKUP_RESTIC_PASSWORD="$K8SBACKUP_RESTIC_PASSWORD" \
  --dry-run=client \
  -o yaml | kubectl apply -f -

kubectl apply -k kubernetes
```

Required environment variables:

- `K8SBACKUP_RESTIC_REPOSITORY`
- `K8SBACKUP_RESTIC_PASSWORD`

The CronJob runs daily at `02:17` in the `k8sbackup` namespace. The image is set
to `k8sbackup-rs:latest`; override it with kustomize for your registry.

## Logging

`k8sbackup-rs` logs structured, human-readable events (start/finish of the
cluster dump, each resource type, and the restic backup/check steps) via
`tracing`. The log level defaults to `info` and can be overridden with the
`RUST_LOG` environment variable, e.g.:

```sh
RUST_LOG=debug cargo run -- --backup-type folder --output backup
```

## Notes

Restic repository URLs printed by the application are sanitized so embedded
passwords are shown as `***`.

Backups use paginated Kubernetes list calls to keep memory usage bounded while
fetching resources.
