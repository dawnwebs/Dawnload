# Architecture decisions

Short records of the decisions that shape cross-platform Dawnload: what the situation was, what
we chose, what else we looked at, and what follows. One file each; a decision that changes gets a
new record that supersedes the old one, rather than an edit.

| # | Decision | Status |
|---|---|---|
| [0001](0001-one-codebase-rust-core-slint-ui.md) | One codebase: a Rust core and a Slint UI | Accepted |
| [0002](0002-core-owns-state-snapshot-and-diffs.md) | The core owns all state; UIs get a snapshot and batched diffs | Accepted |
| [0003](0003-seeding-inside-the-job.md) | Seeding happens inside the job's run; lanes and slots | Accepted |
| [0004](0004-sqlite-state-and-single-instance.md) | SQLite for state, one core per data folder | Accepted |
| [0005](0005-tool-manager.md) | Dawnload installs, verifies and updates its tools | Accepted |
| [0006](0006-built-in-http-engine.md) | A built-in HTTP engine for plain files | Accepted |
| [0007](0007-paste-anything-routing.md) | "Paste anything": routing and the detection line | Accepted |
| [0008](0008-slint-prototype-verdict.md) | Slint prototype: measured, Slint confirmed | Accepted |
| [0009](0009-licensing.md) | Licensing of dependencies and tools | Accepted (owner, 2026-11-10) |

Format: Context, Decision, Alternatives, Consequences. Keep each one under two pages.
