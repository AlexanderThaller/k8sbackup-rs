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

The host name recorded in the snapshot can be set with `--restic-host`,
`K8SBACKUP_RESTIC_HOST`, or `RESTIC_HOST`. It defaults to the system host name,
which inside a Kubernetes pod changes on every run; set it to a stable value so
host-scoped `restic forget` policies work.

## Encryption

The dumped YAML files can be encrypted with [age](https://age-encryption.org)
before they are written, so that neither the local folder nor the restic
repository ever holds plaintext Kubernetes objects (Secrets included):

```sh
age-keygen -o k8sbackup-key.txt
age-keygen -y k8sbackup-key.txt   # the public key to pass below

cargo run -- \
  --backup-type folder \
  --output backup \
  --age-recipient age1...
```

Encrypted objects are written as `<name>.yaml.age` instead of `<name>.yaml` and
are decrypted with the standard age tooling:

```sh
age --decrypt --identity k8sbackup-key.txt backup/default/ConfigMap-v1/example.yaml.age
```

`--age-recipient` can be repeated to encrypt for several recipients, and can be
set with `K8SBACKUP_AGE_RECIPIENT` (comma-separated for multiple keys). Public
keys are parsed at startup, so an invalid key fails before anything is dumped.

Keep the identity file out of the cluster being backed up: the backup cannot be
restored without it.

Note the trade-off when combining age with `--backup-type restic`: age uses a
fresh random file key per file, so an unchanged object encrypts to different
bytes on every run. Restic deduplication and compression therefore stop helping
and each snapshot stores the full backup again. Restic already encrypts the
repository, so this is mainly worth it when the repository itself is untrusted.

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

Optional:

- `K8SBACKUP_AGE_RECIPIENT` — age public key(s) to encrypt the dumped YAML files
  with, see [Encryption](#encryption). Not a secret, so it can be set as a plain
  `env` entry in `kubernetes/cronjob.yaml`.
- `K8SBACKUP_RESTIC_HOST` — set as a plain `env` entry in
  `kubernetes/cronjob.yaml` (default `k8sbackup`). Without it, every run records
  the pod host name as the snapshot host. Because it is set with `env` it takes
  precedence over the same key in the `k8sbackup-restic` secret.

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
