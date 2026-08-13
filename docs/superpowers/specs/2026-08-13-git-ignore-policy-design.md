# Git Ignore Policy Design

## Goal

Keep the repository reproducible and clean without hiding source code or files needed to build the MVP.

## Considered approaches

1. Minimal: keep the current rules only. This is simple but misses common local logs, editor settings, coverage output, and environment files.
2. Balanced (selected): ignore reproducible build output, dependency directories, local editor/OS state, logs, coverage, temporary files, and local environment overrides.
3. Broad: ignore lockfiles and most tool configuration. This reduces tracked files but harms reproducible application builds and can hide important project configuration.

## Rules

- Keep `Cargo.lock` and `package-lock.json` tracked because this repository builds an application and should resolve repeatable dependency versions.
- Ignore Rust, native CMake, TypeScript, and test output that can be regenerated.
- Ignore local `.env` files, but allow a future `.env.example` template to be tracked.
- Ignore local editor and operating-system metadata.
- Do not ignore source directories, manifests, documentation, SDK declarations, native build definitions, or tests.

## Validation

- Confirm no generated or local-only path is currently tracked.
- Check `.gitignore` syntax with `git check-ignore` using representative paths.
- Run `git status --ignored` and verify source and lockfiles remain visible to Git while generated paths are ignored.
