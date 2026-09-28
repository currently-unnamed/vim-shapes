---
name: security-audit
description: Use when asked to check vim-shapes for PII, security vulnerabilities, or malicious/supply-chain packages — before a release/tag, before merging a PR that adds a dependency or touches file import/export/persistence code, or on explicit request ("check for PII", "audit dependencies", "any malicious packages"). Runs three checks scoped to this project's actual attack surface rather than a generic web-app checklist.
---

# Security audit, scoped to what vim-shapes actually is

## Stance

vim-shapes is a local, single-user terminal application: no network server,
no authentication, no database. Most of the standard security checklist
(SQL injection, XSS, JWTs, session management, auth bypass) does not apply
here and raising it is noise. The real attack surface is narrower and
specific to this codebase:

- **Files the user opens that someone else authored** — this app's own save
  format (`persistence.rs`), and the import parsers for other tools'
  formats (`archimate_import.rs`, `foundry_import.rs`) that read
  externally-authored XML/JSON.
- **Paths the user (or a diagram file) supplies for export** —
  `export.rs`, `drawio_export.rs`, `render.rs`'s PNG writer — anywhere a
  file path is built from user input or diagram content and then written to
  or read from.
- **The dependency supply chain** — `Cargo.toml`/`Cargo.lock` — since a
  malicious or compromised crate runs with the same privileges as the app.
- **Data hygiene in examples** — example diagrams, idioms, and fixtures must
  never carry real customer names, account numbers, or other PII.

Run the three checks below independently; they catch different things and
none substitutes for another.

## 1. Dependency vulnerabilities and malicious packages

```
cargo audit
```

(Install once with `brew install cargo-audit` if missing — it pulls the
RustSec advisory database and scans `Cargo.lock`.) A clean run reports zero
vulnerabilities; `unmaintained`-only warnings are advisory, not a CVE — note
them but don't block on them alone.

Then check the supply chain directly, since `cargo audit` only knows about
*known* advisories, not brand-new malicious crates:

```
grep 'source = ' Cargo.lock | sort -u | grep -v 'registry+https://github.com/rust-lang/crates.io-index'
```

Any non-empty result is a dependency pulled from git or a local path rather
than crates.io — that needs manual review of the source, not just a version
bump. When `Cargo.toml` gains a new dependency, sanity-check its name against
typosquatting of a well-known crate, its download count and maintenance
history on crates.io/lib.rs, and that the version pinned in `Cargo.lock`
matches what you intended to add.

## 2. PII and secrets scan

```
git grep -nIE '(api[_-]?key|secret[_-]?key|access[_-]?token|password\s*=|BEGIN (RSA|EC|OPENSSH|PGP) PRIVATE KEY|AKIA[0-9A-Z]{16}|ghp_[A-Za-z0-9]{30,}|sk-[A-Za-z0-9]{20,})' -- .
git grep -nIE '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}' -- .
git ls-files | grep -iE '\.env$|\.pem$|\.key$|credentials|secrets\.'
```

Read every hit — this project's own fixtures, idioms, and manual examples
(`ui/manual.rs`) should only ever use invented names (`AcmeCo`, `Ordering
Service`, etc.), never a real customer, account number, or address. A hit in
`.github/workflows/*` for a generic bot email (e.g. a release-tooling
identity) is not PII and can be ignored; a hit that looks like a real
person's address or a live-looking key is not.

## 3. Vulnerability review of the actual code change

Use the `security-review` skill for the diff-based pass, but read its output
against this project's threat model, not a generic one:

- **Exclude** anything about auth, sessions, SQL/NoSQL injection, XSS, CSRF,
  JWTs — there is no server and no browser surface.
- **Weight heavily** any new file-path handling in `persistence.rs`,
  `export.rs`, `drawio_export.rs`, `archimate_import.rs`,
  `foundry_import.rs` — specifically: can a path from diagram content or an
  imported file escape the intended output directory (path traversal), and
  is any newly-parsed XML/JSON handled without entity expansion limits (XXE)
  or size limits leading to unbounded memory (only flag the latter if it's
  concretely reachable from an untrusted file, per the DOS exclusion
  elsewhere).
- **Weight heavily** any new `std::process::Command`, `sh -c`, or similar —
  this app has none today, so one appearing is itself worth a hard look at
  whether any part of the argument list comes from diagram content.

## How to apply this

Run all three checks together when asked to "check for PII, vulnerabilities,
or malicious packages," before tagging a release, or before merging a PR
that adds a dependency or touches an import/export/persistence file. Report
findings the way `AGENTS.md` asks for reasons elsewhere in this project: say
what's wrong and what to do about it, not just that something looks off.
