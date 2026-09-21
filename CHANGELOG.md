# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.2] - 2026-09-21

### Added

- age encryption for dumped objects: `--age-recipient` /
  `K8SBACKUP_AGE_RECIPIENT` (repeatable) encrypts every YAML file before it is
  written, for both the folder and restic backends. Encrypted objects are
  written as `<name>.yaml.age` and decrypt with the standard age tooling.
  Recipients are parsed at startup, so an invalid key fails before anything is
  dumped.
- `--restic-host` / `K8SBACKUP_RESTIC_HOST` / `RESTIC_HOST` to set the host name
  recorded in the restic snapshot. Previously every CronJob run recorded its pod
  host name, which breaks host-scoped `restic forget` policies.
- Structured logging via `tracing`, with start/finish events for the cluster
  dump, each resource type, and the restic backup/check steps. Level defaults to
  `info` and is configurable with `RUST_LOG`.
- Docker images are also tagged with `git describe --always --tags`
  (e.g. `0.1.1-3-g1a2b3c4`), so a specific commit can be pulled without a
  release.
- BuildKit cache mounts for the cargo registry and build target directory in the
  Docker build, persisted across CI runs with
  `reproducible-containers/buildkit-cache-dance`, so unchanged dependencies are
  not recompiled on every image build.
- MIT LICENSE.txt and the crate metadata (`description`, `repository`, `readme`,
  `keywords`, `categories`) required for publishing to crates.io.

### Changed

- The CronJob runs hourly instead of daily and sets `K8SBACKUP_RESTIC_HOST`.
- Updated and pruned dependencies.

### Fixed

- Pin the container's time zone to UTC so `jiff` (pulled in transitively via
  `k8s-openapi`/`kube`/`opendal`) no longer logs a spurious `WARN` about
  failing to detect the system time zone in the distroless runtime image.
- Docker image workflow's tag-push trigger now matches this repo's actual
  tag naming (`0.1.1`, no `v` prefix); it previously looked for `v*.*.*` and
  never matched, so pushing a version tag never triggered a build.

## [0.1.1] - 2026-07-04

### Added

- Docker image publishing now runs when a GitHub Release is published, in
  addition to existing tag, main branch, pull request, and scheduled workflow
  triggers.

## [0.1.0] - 2026-07-04

### Added

- Initial Kubernetes object backup tool.
- Folder backups that write restore-friendly YAML files.
- Restic-compatible backups through `rustic`, including repository
  initialization and compression.
- Paginated Kubernetes list calls to reduce memory usage while fetching
  resources.
- Restic repository password redaction in command output.
- Deploy-profile `Containerfile` that builds the Rust application and runs it in
  a distroless nonroot runtime image.
- Kustomize manifests for running backups as a Kubernetes CronJob with the
  required namespace, service account, RBAC, and job configuration.
