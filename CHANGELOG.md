# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Pin the container's time zone to UTC so `jiff` (pulled in transitively via
  `k8s-openapi`/`kube`/`opendal`) no longer logs a spurious `WARN` about
  failing to detect the system time zone in the distroless runtime image.
- Docker image workflow's tag-push trigger now matches this repo's actual
  tag naming (`0.1.1`, no `v` prefix); it previously looked for `v*.*.*` and
  never matched, so pushing a version tag never triggered a build.

### Added

- Docker images are now also tagged with `git describe --always --tags`
  (e.g. `0.1.1-3-g1a2b3c4`) on every build, so a specific commit can be
  pulled without waiting for a release.
- Structured logging via `tracing`, with start/finish events for the cluster
  dump, each resource type, and the restic backup/check steps. Log level
  defaults to `info` and is configurable with `RUST_LOG`.
- Add LICENSE.txt specifying the license as MIT.
- Crate metadata (`description`, `repository`, `readme`, `keywords`,
  `categories`) required for publishing to crates.io.
- BuildKit cache mounts for the cargo registry and build target directory in
  the Docker build, persisted across CI runs with
  `reproducible-containers/buildkit-cache-dance`, so unchanged dependencies
  are not recompiled on every image build.

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
