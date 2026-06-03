# Repository Hygiene

This document describes the **large-file policy** for the PoLE
repository and the one-time cleanup procedure that is required
because the early history contains a 162 MB Windows binary
(`chain/poled.exe`).

## Policy

| Rule | Why |
|---|---|
| **No compiled binaries in the working tree** | Each release artifact is a GitHub Release; the source repo should clone in < 30 s. |
| **No Go build output** (`chain/build/`, `chain/dist/`, `chain/.tmp-poled-home/`) | Reproducible builds need a clean tree. |
| **No bundled toolchains** (`tools/wix*.zip`) | Re-download from the upstream WiX release when needed. |
| **No IDE / OS metadata** | Handled by `.gitignore`. |
| **No secrets** | `.env`, `*.pem`, `*.key`, `id_rsa`, `id_ed25519` are listed in `.gitignore`. |

The authoritative list lives in `.gitignore`.  When in doubt,
**don't commit; the pre-commit hook / CI will reject it anyway.**

## Why this document exists

The commit history contains:

| Commit | Bad object |
|---|---|
| `49ac629` 初始化 PoLE V1 项目代码库 | `chain/poled.exe` (162.16 MB) |
| `7a96aab` security: fix critical issues from security audit | same file (re-merged) |

GitHub's pre-receive hook rejects pushes with files > 100 MB, so
the repo cannot be force-pushed as-is.  The `*.exe` rule in
`.gitignore` is too late to help — the file is already in the
object graph, and `.gitignore` only blocks *new* adds.

## One-time cleanup

Use the provided script:

```bash
# Step 1 — preview what gets stripped (no changes to the working repo)
./scripts/strip-large-files-from-history.sh --dry-run

# Step 2 — actually do it (creates a backup branch, rewrites history)
./scripts/strip-large-files-from-history.sh --apply

# Step 3 — sanity-check
cargo build --release
cargo test

# Step 4 — force-push
git push origin main --force

# Step 5 — tell collaborators to re-clone
#   rm -rf pole--1
#   git clone https://github.com/q3874758/pole--1.git
```

The script:

1. Refuses to run on a dirty tree.
2. Creates a `backup/pre-history-strip-<timestamp>` branch
   *before* the rewrite.
3. Runs `git-filter-repo` with the agreed path list.
4. Prints the resulting top-10 commit log and the largest
   5 files remaining.
5. **Does not push** — the operator reviews the result and
   pushes manually.

## Recovery

If the rewrite breaks something:

```bash
# The backup branch is still there for at least 30 days.
git checkout backup/pre-history-strip-<timestamp>
git push origin backup/pre-history-strip-<timestamp>
# Tell collaborators to fetch it, then rewind HEAD on main.
```

If you need to inspect a single file from a previous state:

```bash
git show backup/pre-history-strip-<timestamp>:chain/poled.exe > /tmp/poled.exe
# (you'll need the backup branch — git-filter-repo deletes the
#  unreachable objects after the default 30-day reflog expiry)
```

## Going forward

- The `package.exclude` block in `Cargo.toml` makes `cargo
  package` skip the same paths.
- The CI `rust` job runs `cargo fmt` + `cargo clippy -D warnings`
  on every push, so accidental new artifacts are caught at
  push time.
- A future `pre-commit` framework (see TODO list) can run
  `du -sh` on staged paths and reject anything > 5 MB.

## References

- `git-filter-repo` manual: <https://github.com/newren/git-filter-repo>
- GitHub file size limit: <https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github>
- Sigstore / cosign SBOM signing: see [SBOM.md](SBOM.md) (TODO)
