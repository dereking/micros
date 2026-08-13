# Git Ignore Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep generated artifacts and machine-local files out of Git while retaining reproducibility files and project sources.

**Architecture:** `.gitignore` remains the single policy file. Representative nonexistent paths are tested with `git check-ignore`, and the tracked-file index is audited separately so the policy cannot conceal an already-committed artifact.

**Tech Stack:** Git ignore patterns, shell-based validation

---

### Task 1: Expand and validate repository ignore rules

**Files:**
- Modify: `.gitignore`
- Reference: `docs/superpowers/specs/2026-08-13-git-ignore-policy-design.md`

- [ ] **Step 1: Prove the new categories are not covered yet**

Run:

```bash
printf '%s\n' .env .env.local .vscode/settings.json coverage/lcov.info micro.log scratch.tmp | git check-ignore --stdin
```

Expected: exit status `1` with no matching paths.

- [ ] **Step 2: Add the balanced ignore policy**

Replace `.gitignore` with these categorized rules:

```gitignore
# Operating-system metadata
.DS_Store
Thumbs.db

# Local agent and editor state
.superpowers/
.idea/
.vscode/
*.swp
*.swo
*~

# Local environment overrides; templates remain trackable
.env
.env.*
!.env.example

# Rust and native build output
target/
debug/
native/build/
cmake-build-*/
**/*.rs.bk
*.pdb
**/mutants.out*/

# Node and TypeScript output
node_modules/
apps/*/dist/
dist/
*.tsbuildinfo
.npm/

# Runtime and test output
*.mbc
coverage/
*.profraw
*.profdata
*.log
npm-debug.log*
yarn-debug.log*
yarn-error.log*
pnpm-debug.log*

# Temporary files
tmp/
temp/
*.tmp
*.bak
```

- [ ] **Step 3: Verify representative matches and the template exception**

Run:

```bash
printf '%s\n' .env .env.local .vscode/settings.json coverage/lcov.info micro.log scratch.tmp target/debug/app node_modules/pkg/index.js apps/counter/dist/app.mbc | git check-ignore --stdin
git check-ignore .env.example
```

Expected: the first command prints all nine paths; the second exits `1`, proving `.env.example` remains trackable.

- [ ] **Step 4: Audit the index and syntax**

Run:

```bash
git diff --check
git ls-files | rg '(^|/)(target|node_modules|dist|build|coverage|\.idea|\.vscode)(/|$)|\.(mbc|log|tmp|profraw|profdata)$'
git status --short --ignored
```

Expected: `git diff --check` succeeds; the tracked-file scan prints nothing; generated local directories appear with `!!` and no source or lockfile is ignored.

- [ ] **Step 5: Commit and publish**

Run:

```bash
git add .gitignore docs/superpowers/plans/2026-08-13-git-ignore-policy.md
git commit -m "chore: expand generated file ignores"
git push origin master
```

Expected: one commit is created and `origin/master` advances to that commit.
