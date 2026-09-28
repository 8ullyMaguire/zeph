# Zeph — Build-from-Scratch Specification & Implementation Plan

**Status:** Draft for review — no feature has been approved.
**Written:** 2026-09-28
**Subject:** `https://github.com/bug-ops/zeph` @ `4af821b1` ("chore(deps): update rust crate dirs to v7 (#6797)", 2026-09-22), version **0.22.5**
**Audience:** an LLM implementing this with no prior knowledge of the repository.

> **Read this first.** Zeph is not a young project with an empty roadmap — it is a mature,
> heavily-specified codebase (89 specs, a constitution, 16,110 tests). The interesting
> work is *narrow*. Section 3 records what is already built, because most of what a
> newcomer would re-plan is already done. Section 4 identifies the one gap that is real,
> verified against the tree. Sections 5–8 specify it completely.
>
> **This repository mandates spec-first development.** `AGENTS.md` and
> `specs/constitution.md` require reading the relevant spec in `specs/` and
> `specs/001-system-invariants/spec.md` before implementing anything. §9 covers what that
> means for an implementer here.

---

## 1. How to read this document

| If you are… | Read |
|---|---|
| A new implementer with zero context | §2 (what exists) → §3 (what is already built) → §5 onward |
| Reviewing before approving scope | §1, §4, §10 (phases), §11 (open questions) |
| Implementing Phase 1 | §5 in full, plus §9 (repo conventions you must follow) |
| Implementing later phases | The phase section referenced in §10 |

---

## 2. The system as it exists

### 2.1 What Zeph is

A memory-first AI agent for long-running work — local, cloud, or decentralized. The
organising idea is persistence: SQLite conversation history, semantic recall, a typed
knowledge graph, session digests, and goal-aware compaction, so an agent survives across
days rather than losing everything when a window closes. It ships as a single ~12 MB Rust
binary with no Python, no Node, and no database server to run.

Licensed **MIT OR Apache-2.0** (dual). MSRV **1.98**, edition **2024**.

### 2.2 Verified repository facts

Measured on the clone, not read from documentation.

| Property | Value | How to re-measure |
|---|---|---|
| Version | 0.22.5 | `grep -m1 '^version' Cargo.toml` |
| Edition / MSRV | 2024 / 1.98 | `grep -E 'edition\|rust-version' Cargo.toml` |
| Licence | MIT OR Apache-2.0 | `head -3 LICENSE` |
| HEAD | `4af821b1` 2026-09-22 | `git log -1` |
| Workspace members | 32 crates | `ls -d crates/*/ \| wc -l` |
| Rust files | 1,267 | `git ls-files '*.rs' \| wc -l` |
| Total Rust LOC | 728,424 | `git ls-files '*.rs' \| xargs wc -l \| tail -1` |
| Specs | 89 | `ls -d specs/*/ \| wc -l` |
| Test functions | **16,110** | `grep -rc '#\[test\]\|#\[tokio::test\]'` over all `.rs` |
| Test files | 886 | files containing at least one test attribute |
| Build | clean, 54.07 s for `-p zeph-llm -p zeph-skills` | see §2.7 |

The README badge claims **15,761** tests against a measured **16,110**. That is the badge
lagging behind reality by ~350, not a documentation failure — the count grows continuously
and the badge is only refreshed at release. Unlike a typical young repo, Zeph's prose
documentation is accurate; its *spec status fields* are what go stale (§3.1).

### 2.3 Crate map

32 crates. Size order, largest first:

| Crate | LOC | Responsibility |
|---|---|---|
| `zeph-core` | 131,853 | Agent loop, turn orchestration, tool execution, speculative decoding |
| `zeph-memory` | 87,285 | SQLite/Postgres stores, usage ledger, graph memory, semantic recall |
| `zeph-llm` | 49,871 | Provider trait, per-provider clients, router/triage, usage tracking |
| `zeph-tools` | 46,827 | Built-in tool implementations, sandboxed execution |
| `zeph-config` | 42,220 | Config structs, loading, migration, validation |
| `zeph-tui` | 38,271 | Terminal UI, slash commands, command palette, widgets |
| `zeph-subagent` | 26,866 | Sub-agent lifecycle, peer messaging, context propagation |
| `zeph-orchestration` | 24,217 | Task graph, planner, aggregator, ensemble |
| `zeph-acp` | 21,360 | Agent Client Protocol (editor integration) |
| `zeph-skills` | 18,824 | `SKILL.md` manifests, skill state, bundled skills |
| `zeph-mcp` | 18,304 | MCP client/server, multimodal passthrough |
| `zeph-durable` | 14,026 | Durable execution, replay, fingerprints |
| `zeph-sanitizer` | 12,298 | Content sanitising, `MediaSanitizer` |
| `zeph-channels` | 12,264 | Telegram, Discord, Slack |
| `zeph-common` | 11,473 | Shared primitives |
| `zeph-context` | 10,896 | Context budget, compaction, microcompact |
| `zeph-agent-context` | 10,271 | Ambient shared-state context reads |
| `zeph-a2a` | 9,209 | Agent-to-agent protocol |
| `zeph-commands` | 9,111 | Slash command registry |
| `zeph-plugins` | 8,863 | Plugin host |
| `zeph-index` | — | Code indexing / AST retrieval |
| `zeph-gateway` | — | HTTP gateway (`/webhook`, `/metrics`, health) |
| `zeph-scheduler` | — | Cron and trigger scheduling |
| `zeph-session` | — | Session store |
| `zeph-vault` | — | age-encrypted secret store |
| `zeph-worktree` | — | Git worktree isolation |
| `zeph-agent-tools` | — | Agent-facing tool surface |
| `zeph-agent-persistence` | — | Async persistence pipeline |
| `zeph-agent-feedback` | — | Implicit correction detection |
| `zeph-db` | — | Migrations, SQLite + PostgreSQL dialects |
| `zeph-bench` | — | Benchmark harness |

### 2.4 The provider trait (`zeph-llm`)

`crates/zeph-llm/src/provider_dyn.rs` defines `LlmProvider`, the single extension point for
model access. Its methods, in declaration order:

```
context_window, chat, chat_stream, supports_streaming, embed, embed_batch,
supports_embeddings, name, model_identifier, effective_model_identifier,
supports_vision, supports_tool_use, chat_with_tools, last_cache_usage, last_usage,
last_reasoning_tokens, take_compaction_summary, chat_with_extras,
debug_request_json, list_models, supports_structured_output
```

`supports_vision` at `provider_dyn.rs:128` is the subject of §5.

Sixteen concrete provider implementations of the trait method exist across the workspace
(`git grep -n '    fn supports_vision(&self) -> bool' -- 'crates/*/src/*' | wc -l` → 16;
a further ~21 hits are tests and delegation helpers). The relevant ones for §5, with their
**current** `supports_vision` bodies:

| Implementation | File:line | Current behaviour |
|---|---|---|
| `OpenAiProvider` | `openai/mod.rs:1101` | `vision_override` → else `model_supports_vision(&self.model)` |
| `CompatibleProvider` | `compatible.rs:376` | delegates to `self.inner.supports_vision()` |
| `ClaudeProvider` | `claude/mod.rs:1451` | **`true`, unconditionally** |
| `GeminiProvider` | `gemini/mod.rs:1405` | **`true`, unconditionally** |
| `OllamaProvider` | `ollama.rs:248` | `self.capabilities.iter().any(\|c\| c == "vision")` — server-reported |
| `CandleProvider` | `candle_provider/mod.rs:342` | `false` |
| `TriageRouter` | `router/triage.rs:695` | `tier_providers.iter().any(\|(_, p)\| p.supports_vision())` |
| `RouterProvider` | `router/provider_impl.rs:320` | `providers.iter().any(LlmProvider::supports_vision)` |
| `AnyProvider` | `any.rs:788` | `delegate_provider!` macro to the active variant |
| `MaskedProvider` | `masking.rs:298` | delegates to inner |
| `MockProvider` | `mock.rs:32` | configurable, defaults `false` |
| Cocoon / Gonka / SubProvider | various | see `git grep -n 'fn supports_vision'` |

### 2.5 The existing per-model table (the pattern to generalise)

`openai/mod.rs:1885-1904` is the reference implementation of a per-model capability
table, and §5 follows its shape exactly:

```rust
fn model_supports_vision(model: &str) -> bool {
    if model.starts_with("gpt-4o")
        || model.starts_with("gpt-4-turbo")
        || model.starts_with("gpt-4-vision")
        || model.starts_with("gpt-4.1")
        || model.starts_with("gpt-5")
    { return true; }
    if is_text_only_o_series(model) { return false; }
    starts_with_o_digit(model)
}

fn is_text_only_o_series(model: &str) -> bool {
    model.starts_with("o1-mini") || model.starts_with("o1-preview") || model.starts_with("o3-mini")
}
```

The doc comment records *why* the exclusions exist: vision support is not uniform across
the `o`+digit reasoning family, and reusing the shared `starts_with_o_digit` prefix check
wholesale would reintroduce bug #6411 for three text-only variants. This is the lesson
§5 must not repeat.

### 2.6 Consumers of `supports_vision`

Only two real decision sites, both in `zeph-core`:

- `crates/zeph-core/src/agent/mod.rs:1887` — `if !all_image_parts.is_empty() && self.provider.supports_vision()`
- `crates/zeph-core/src/agent/tool_execution/tool_result.rs:556` — `if !self.provider.supports_vision() { … }`, gating sibling-Image emission (the "C3" safety net named in spec 072 §7)

Everything else is delegation, a test, or a changelog entry. 164 textual references total,
the overwhelming majority historical.

### 2.7 Build and toolchain

```
rustc 1.98.1 (48a229cea 2026-09-01) (Arch Linux)
cargo 1.98.1
```

The local toolchain is **exactly MSRV 1.98.1**, which is convenient: a build that passes
here satisfies the project's MSRV gate.

> ### ⚠ Build location — read before running any cargo command
>
> The canonical clone is at `~/code/rust/zeph`, which resolves through a symlink to an
> **sshfs network mount** (`~/mnt/thinkcentre/personal/documents/code/rust`, `fuse.sshfs`).
>
> **The build fails there.** `crates/zeph-skills/build.rs:8` asserts that
> `crates/zeph-skills/skills` resolves, and that path is a git-tracked **symlink** to
> `../../.zeph/skills`. The sshfs mount does not support symlink resolution:
> `readlink` on a symlink there returns an empty string and `stat` fails with
> `Operation not permitted`. Verified across all three mounted shares (`/personal`,
> `/disk-important`, `/media`); the same symlink on local disk resolves correctly. This is
> intermittent-looking but is not — a symlink created in one directory may read back while
> one in another does not, which makes it look flaky. Assume any symlink on the mount is
> unusable.
>
> **A local working clone exists and builds cleanly:**
> ```
> /home/alvaro/code-local/rust/zeph        # HEAD 4af821b1, symlinks resolve (28 skills)
> ```
> Use it for all building. `cargo check -p zeph-llm -p zeph-skills` there finished in
> **54.07 s with zero warnings**. Set `CARGO_TARGET_DIR` off the network mount:
> ```
> export CARGO_TARGET_DIR=/home/alvaro/.cargo-target/zeph
> ```
>
> `cargo nextest` is **not installed** locally, though `AGENTS.md` names it the default
> runner. Either `cargo install cargo-nextest` or fall back to `cargo test`.

---

## 3. What is already built — do not re-plan it

This is the section that prevents wasted work. Zeph tracks its work in `specs/`, and the
temptation is to read a `status: draft` spec and build it. **In this repo that is usually
wrong**, because specs are written before implementation and their status field is
updated far less often than the code changes.

### 3.1 The draft-spec trap

89 specs by status:

| Status | Count |
|---|---|
| `approved` | 62 |
| `implemented` | 14 |
| `draft` | **11** |
| none | 1 |

Of the 11 `draft` specs, at least five are substantially or fully implemented. Verified
by checking the tree, not by reading the status field:

| Draft spec | Claimed | Actually in the tree |
|---|---|---|
| `082-per-message-usage-cost-tracking` | draft | **Implemented.** Migration `115_usage_records.sql` exists for both SQLite and Postgres; `zeph-memory/src/store/usage_records.rs`; `Agent::build_usage_record` in `zeph-core/src/agent/plan.rs:1020,1194`; `UsageTracker::last_ttft` at `zeph-llm/src/usage.rs:29` already cites issue #6549. Only the out-of-scope TUI display remains. |
| `072-multimodal-mcp-passthrough` | draft | **Implemented.** `zeph-sanitizer/src/media.rs` (`MediaSanitizer`), `media_passthrough` config in `zeph-config`, `--init` and `--migrate-config` steps. A residual `TODO(#5366)` remains — see §4. |
| `076-cli-init-migrate-config-flag-mismatch` | draft | **Implemented.** `--migrate-config` wired and referenced across `CHANGELOG.md`, `zeph-config`, `AGENTS.md`. |
| `067-knowledge-ingest` | draft | **Substantially implemented.** 14 files reference `knowledge_ingest`, 6 have an `ingest.rs`. |
| `086-skill-execution-state` | draft | **Partially implemented**, self-documented: "orchestration half refuted (no node transcript exists), subagent half deferred". |

**Rule for this repo: never trust a spec's `status:` field or a `TODO` comment. Verify
against the tree with `git grep`, then update the spec.**

### 3.2 What the constitution requires

`specs/constitution.md` and `specs/001-system-invariants/spec.md` are mandatory reading.
Non-negotiable rules that constrain any implementation here:

- Read the relevant spec **and** the system invariants before writing code.
- Violating a `## Key Invariants` or `NEVER` section requires an explicit architectural
  decision, not merely a code review.
- Do not create git worktrees inside the repository; use `../worktrees/`.
- All secrets live in the Zeph age vault — never environment variables or `.env`.
- A background operation in the TUI must have a visible spinner with a status message.
- No new `tokio::spawn()` call sites; route async work through a named `TaskSupervisor`
  service (see spec 082 §3 for the phrasing).

---

## 4. The one real gap: per-model vision capability

Having eliminated the stale drafts, one genuine, precisely-bounded gap remains. It is
named by the repository's own code:

```html
<!-- TODO(#5366): per-model vision-capability table replaces coarse
     supports_vision() boolean; see specs/072 §7 -->
```
— `specs/072-multimodal-mcp-passthrough/spec.md:399`

and, in the working tree, the two router implementations document the resulting
behaviour honestly:

```rust
/// Aggregate: `true` if any configured provider supports vision. This is a coarse,
/// optimistic signal (mirrors `TriageRouter::supports_vision`'s known v1 limitation,
/// spec-072 §7) …
fn supports_vision(&self) -> bool {
    self.state.providers.iter().any(LlmProvider::supports_vision)
}
```
— `router/provider_impl.rs:315-325`

### 4.1 The problem, stated precisely

`supports_vision()` answers "can this provider accept an image?" with a single boolean for
the whole provider. For `OpenAiProvider` that is nearly right — it consults a per-model
prefix table (§2.5). For **`ClaudeProvider` and `GeminiProvider` it is wrong by
construction**: both return a hardcoded `true` regardless of which model is configured.

Consequences, in order of severity:

1. **Silent misrouting.** `tool_result.rs:556` gates sibling-Image emission on
   `supports_vision()`. With a Claude or Gemini provider that reports `true`, images are
   attached to a turn that the configured model cannot process — producing a server-side
   error at best, a silently degraded response at worst.
2. **No override escape hatch.** `OpenAiProvider` has `OpenAiConfig::vision` /
   `with_vision()`. `CompatibleProvider` forwards it to its inner provider
   (`compatible.rs:376`). Claude and Gemini have **no equivalent**, so a user on a
   text-only Claude or Gemini variant has no way to correct the report.
3. **Router aggregation is optimistic by design and stays that way.** `RouterProvider` and
   `TriageRouter` report "any member supports vision", which is correct for a *routing
   eligibility* question but wrong for a *"will this call succeed"* question. §5.4
   specifies how to keep these honest without changing their contract.

### 4.2 What already exists to build on

- **The pattern:** `model_supports_vision` + `is_text_only_o_series` in `openai/mod.rs`
  (§2.5), with 15 accompanying tests in `openai/tests.rs:113-305`.
- **The override precedent:** `OpenAiConfig::vision` field and `with_vision()` builder
  (`compatible.rs:204-228` documents the delegation contract).
- **The precedent for exclusions:** #6411 — a prefix match that was too broad and shipped
  a bug. It is why `is_text_only_o_series` exists and why §5.4 requires explicit exclusion
  tests.
- **The Ollama model:** server-reported capabilities, already per-model
  (`ollama.rs:248`). It is the design endpoint, not something to change.

---

## 5. Requirement — per-model vision capability table

### 5.1 The want

`supports_vision()` should answer per model, not per provider, and every provider that
hardcodes the answer should gain an explicit override — so an image is never attached to a
model that cannot read it, and a user can always correct a wrong report.

### 5.2 Design

**Constraints taken from the existing code, not invented:**

- **Fail-closed.** An unrecognised model returns `false`, exactly as
  `OllamaProvider::supports_vision` and `OpenAiProvider::supports_vision` already do. The
  `openai/mod.rs:1096-1100` doc comment states the rationale: an arbitrary
  `type = "compatible"` endpoint has a model name with no naming convention, so assuming
  vision "would risk silently sending an image the model cannot process."
- **Per-provider tables, not one global table.** Model names are not namespaced across
  providers, and each provider's naming scheme is independent. A shared table would be
  wrong for at least one of them.
- **Override wins, always.** The existing precedence is `override → table`. Preserve it.
- **No new dependency.** This is pure string matching, as `openai/mod.rs` already is.
- **No change to the trait signature.** `supports_vision(&self) -> bool` stays. Changing
  it would ripple through 16 implementations, two routers, and the ACP capability
  advertisement for no gain — the information is already reachable via `self.model`.

### 5.3 Changes, file by file

**New: `crates/zeph-llm/src/capabilities.rs`**

A small module holding the per-provider tables, so the knowledge is in one place rather
than scattered across provider modules:

```rust
//! Per-model capability tables.
//!
//! Fail-closed: an unrecognised model reports `false`. Guessing `true` risks
//! attaching an image to a model that cannot process it (see #6411 for the
//! inverse bug — an over-broad prefix match that shipped).

/// Claude models that accept image input. Text-only variants are listed
/// explicitly and must be matched *before* any prefix rule.
pub fn claude_supports_vision(model: &str) -> bool { … }

/// Gemini models that accept image input.
pub fn gemini_supports_vision(model: &str) -> bool { … }
```

Rules for both functions, mirroring `openai/mod.rs`:
1. An explicit text-only exclusion list is checked **first**.
2. Then a vision-capable prefix list.
3. Default `false`.
4. Both carry a doc comment naming the issue that motivated any exclusion.

**Change: `claude/mod.rs:1451`** — replace `true` with
`self.vision_override.unwrap_or_else(|| capabilities::claude_supports_vision(&self.model))`.

**Change: `gemini/mod.rs:1405`** — the same shape with `gemini_supports_vision`.

**Change: `openai/mod.rs`** — move `model_supports_vision` and `is_text_only_o_series`
into `capabilities.rs` unchanged, and re-export from `openai/mod.rs` so the 15 existing
tests in `openai/tests.rs` keep compiling. **This is a pure move; do not "improve" the
table while moving it** — that would conflate a refactor with a behaviour change.

**Add: override fields**, following `OpenAiConfig::vision`:
- `ClaudeConfig::vision: Option<bool>` + `ClaudeProvider::with_vision(bool)`
- `GeminiConfig::vision: Option<bool>` + `GeminiProvider::with_vision(bool)`

**Change: `provider.rs:920`** — the trait's default `supports_vision` stays `false`.

**Change: routers (§5.4).**

**Config:** no new `config/default.toml` keys. The override is a constructor-level
builder, consistent with `OpenAiConfig::vision`, which is not a config key either.

### 5.4 Router semantics — the subtle part

`RouterProvider` and `TriageRouter` aggregate with `.any()`. That is **correct** for
eligibility ("could some member handle an image?") and **wrong** for a pre-dispatch
guarantee. Do not change their existing `supports_vision()` — that would break routing.

Instead, the guarantee moves to the dispatch site, where spec 072 already anticipated it
("C3"): `router/provider_impl.rs`'s `chat_with_tools` already applies "the real
per-provider safety net (C3) so an image is never sent to a provider that individually
reports `supports_vision() == false`". Verify that net exists and is exhaustive; if it does
not, closing this gap is what makes it sound. The test in §5.5 item 8 is the check.

### 5.5 Acceptance criteria

Observable, each checkable from outside:

1. `cargo check -p zeph-llm` and `cargo clippy -p zeph-llm --all-targets -- -D warnings`
   both pass with zero warnings.
2. `ClaudeProvider::new("claude-3-5-haiku-20241022")` — a text-only variant — reports
   `supports_vision() == false`; `claude-sonnet-4-*` reports `true`. **Two tests,
   opposite outcomes.** If the exact current model names differ, pin the ones the tree's
   own `list_models()` returns.
3. Same pair of tests for Gemini, including at least one `*-flash-lite` or other
   text-only variant if one exists.
4. `with_vision(true)` forces `true` and `with_vision(false)` forces `false` **even for a
   model the table would answer the other way** — 4 tests (2 providers × 2 directions).
5. An unrecognised model string (`"not-a-real-model"`) reports `false` for every provider
   — 3 tests, one per changed provider.
6. The 15 existing `openai/tests.rs` vision tests still pass **unmodified** after the
   table moves. No test edits in that file.
7. `zeph-llm` test count increases by exactly the number of tests added; the workspace
   total goes from **16,110** to 16,110 + N. State N in the PR.
8. A router with one vision-capable and one text-only member, given an image, sends the
   image **only** to the capable member (assert via `mockito` request inspection).
9. No new entry in `[dependencies]` of `crates/zeph-llm/Cargo.toml`; `cargo tree -p zeph-llm`
   output is unchanged.
10. `CHANGELOG.md` `[Unreleased]` gains an entry under `fix(llm)`; `specs/072` loses the
    `TODO(#5366)` comment; spec 003's `supports_vision` entry (§69) is updated to describe
    per-model resolution.

---

## 6. Documentation debt (do this in the same PR)

The §3.1 finding is itself a defect worth fixing, and it is cheap:

- **Update the stale `status:` fields.** 082, 072 and 076 are `draft` but implemented.
  Set them to `implemented` with a date, per the convention in `specs/README.md`.
- **Update the test-count badge** in `README.md` from 15,761 to the true 16,110.
- **Remove the `TODO(#5366)`** from `specs/072/spec.md:399` as part of §5.

This matters beyond tidiness: an agent following `AGENTS.md` will read a `draft` spec and
rebuild shipped functionality. That is the single most expensive mistake available in this
repo.

---

## 7. Cross-cutting requirements

1. **MSRV 1.98 and edition 2024.** The local toolchain is exactly 1.98.1, so any local
   pass satisfies the gate.
2. **Zero clippy warnings**, `AGENTS.md`'s explicit standard.
3. **No `unwrap`/`expect` in production code** where error propagation is possible.
4. **No new `tokio::spawn()`**, no new dependency, no new config key without a
   `--init` wizard step and a `--migrate-config` step (`AGENTS.md` integration list).
5. **Docs and changelog are part of the change.** `CHANGELOG.md [Unreleased]`, the affected
   spec, and `book/src/advanced/orchestrator.md:53` (which documents
   `supports_vision()`) all need updates.
6. **Integration points are mandatory** for new functionality per `AGENTS.md`: config
   section, CLI argument, TUI command-palette entry, `--init` step, `--migrate-config`
   step, a live-testing playbook in `.local/testing/playbooks/`, and a row in
   `.local/testing/coverage-status.md`. For §5 most of these do not apply (no new
   user-facing surface), but the changelog, spec, and book entries do.

---

## 8. Verification

The gate, run from the **local** clone (§2.7):

```bash
cd /home/alvaro/code-local/rust/zeph
export CARGO_TARGET_DIR=/home/alvaro/.cargo-target/zeph

cargo check -p zeph-llm
cargo clippy -p zeph-llm --all-targets -- -D warnings
cargo test -p zeph-llm                     # or: cargo nextest run -p zeph-llm

# full pre-commit gate from AGENTS.md
cargo +nightly fmt --check
cargo check --workspace --features full
cargo clippy --profile ci --workspace --features full -- -D warnings
cargo nextest run --config-file .github/nextest.toml --workspace --profile ci --features full
```

Baseline measured 2026-09-28: `cargo check -p zeph-llm -p zeph-skills` finished in 54.07 s,
zero warnings, rustc 1.98.1.

Live end-to-end validation is **not optional** for agent-loop or user-facing changes per
`AGENTS.md`, but §5 is a pure capability-reporting change with no turn-loop semantics, so
`cargo test -p zeph-llm` plus the C3 router test is the proportionate gate. Override
`logging.file`, `memory.sqlite_path` and `debug.output_dir` to `/tmp/zeph-ci/<timestamp>/`
for any live run.

---

## 9. Repository conventions an implementer must follow

Condensed from `AGENTS.md` and `specs/constitution.md`. Violating these is a review
rejection even when the code is correct.

- **Spec-first.** Read `specs/001-system-invariants/spec.md` and the relevant subsystem
  spec before implementing. Update the spec if it is wrong — that is expected, not optional.
- **Crate-first verification.** `cargo build -p <crate>`, then
  `cargo nextest run -p <crate>`; workspace-wide only when the change crosses boundaries.
- **Test runner is `cargo nextest`.** Not installed locally (§2.7).
- **Tests live inline** in `#[cfg(test)] mod tests` at the bottom of the file, or in the
  crate's `tests/` directory. 886 of 1,267 files contain tests.
- **Name integration tests `*_integration.rs`.** Add a regression test for every bug fix.
- **Conventional Commits with scopes:** `feat(zeph-tools): …`, `fix(llm): …`,
  `fix: …`, `ci: …`. Reference issues as `(#5366)`.
- **No worktrees inside the repo** — use `../worktrees/`.
- **Secrets only in the age vault**, never env vars or `.env` files.
- **No model names hardcoded outside a capability table** — providers are referenced by
  name via a single `*_provider` field (`AGENTS.md`, multi-model design). This is why §5's
  tables live in one module rather than being scattered.

---

## 10. Phased delivery order

| Phase | Work | Why here |
|---|---|---|
| **1** | §5 per-model vision capability (5.3) | The only verified-open functional gap. Self-contained: one new module, two changed functions, no signature change, no new dependency. |
| **2** | §5.4 router C3 verification | Depends on Phase 1's per-model answers being correct. Acceptance criterion 8 is the test. |
| **3** | §6 documentation debt | Cheap, and it prevents the §3.1 trap from costing the next agent a full cycle. Can ride along with Phase 1. |
| — | 049, 050, 055, 067, 068, 069, 074, 086 | Remaining draft specs. **Not scheduled**: each needs its own tree-verification pass first, exactly as §5.4 and §3.1 did for this one. Several may already be implemented. |

Phase 3 is listed third but is a 20-minute change; do it first if the goal is unblocking
other agents.

---

## 11. Open questions

### Blocking — answer before starting

1. **Which Claude/Gemini model names go in the tables?** Model lineups move fast, and a
   wrong entry either blocks a capable model or permits a text-only one. *Recommended:
   derive the lists from each provider's own `list_models()` implementation in the tree
   (`gemini/mod.rs:1413` has one), and pin them with tests that fail when a name is
   unknown.* Alternative — treat "unknown Claude/Gemini model" as `true`, preserving today's
   behaviour — is rejected: it contradicts the fail-closed principle in §5.2 and reintroduces
   exactly the silent-misrouting failure this change exists to prevent.
2. **Does the override need to be a config key, not just a builder method?** `OpenAiConfig::vision`
   is builder-only, but a user on Claude may have no way to set a builder. *Recommended:
   builder-only for v1, matching the existing precedent; a config key arrives with a
   `--init` + `--migrate-config` step if it proves necessary.*
3. **Should `CandleProvider`'s hardcoded `false` stay?** Some Candle-backed models
   (multimodal checkpoints) may accept images. *Recommended: leave `false` — out of scope,
   and fail-closed is correct for an in-process local backend.*

### Deferred — decide during the phase

4. Whether to promote `capabilities.rs` tables to a generated data file (e.g. refreshed
   from provider manifests) rather than hand-maintained string tables.
5. Whether `supports_vision` should gain a sibling `supports_audio` / `supports_pdf` as
   MCP passthrough widens (the `TODO(#5366)` in `zeph-mcp/src/content.rs:61` defers
   Audio/blob/resource-link for the same structural reason).
6. Whether spec 003 (`003-llm-providers`) should be the canonical home for the capability
   tables' documentation, given it already documents the trait method at §69.

---

## 12. Appendix — reference commands

```bash
# repository facts
git log -1 --format='%h %ci %s'
grep -m1 '^version' Cargo.toml
ls -d crates/*/ | wc -l                          # 32
ls -d specs/*/ | wc -l                           # 89
git ls-files '*.rs' | xargs wc -l | tail -1     # 728424

# the real test count (not the badge)
for f in $(git ls-files '*.rs'); do
  grep -cE '#\[test\]|#\[tokio::test\]' "$f"
done | awk '{s+=$1} END {print s}'                # 16110

# spec status histogram — then verify each draft against the tree
grep -h '^status:' specs/*/spec.md | sort | uniq -c

# §5 surface area
git grep -n '    fn supports_vision(&self) -> bool' -- 'crates/*/src/*'   # 16 impls
sed -n '1885,1904p' crates/zeph-llm/src/openai/mod.rs   # the pattern to copy
sed -n '127,129p' crates/zeph-llm/src/provider_dyn.rs  # the trait method
git grep -n 'TODO(#5366)'                                # the residual gap

# consumers
sed -n '1887p' crates/zeph-core/src/agent/mod.rs
sed -n '556p'  crates/zeph-core/src/agent/tool_execution/tool_result.rs

# build (LOCAL clone only — see §2.7)
cd /home/alvaro/code-local/rust/zeph
export CARGO_TARGET_DIR=/home/alvaro/.cargo-target/zeph
cargo check -p zeph-llm
```

### Where everything lives

| Artefact | Path |
|---|---|
| Workspace manifest, features | `Cargo.toml` |
| Agent instructions (**mandatory**) | `AGENTS.md` |
| Project constitution | `specs/constitution.md` |
| System invariants (**mandatory**) | `specs/001-system-invariants/spec.md` |
| Spec index / MOC | `specs/README.md`, `specs/MOC-specs.md` |
| Provider trait | `crates/zeph-llm/src/provider_dyn.rs` |
| Vision tables (**new**) | `crates/zeph-llm/src/capabilities.rs` |
| Existing table to generalise | `crates/zeph-llm/src/openai/mod.rs:1885` |
| Router aggregation | `crates/zeph-llm/src/router/{provider_impl.rs,triage.rs}` |
| Runtime consumers | `crates/zeph-core/src/agent/mod.rs:1887`, `.../tool_result.rs:556` |
| Migrations (SQLite + Postgres) | `crates/zeph-db/migrations/{sqlite,postgres}/` |
| Config + migration steps | `crates/zeph-config/`, `config/default.toml` |
| mdBook documentation | `book/src/` |
| This document | `docs/SPEC-IMPLEMENTATION-PLAN.md` |
