# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
