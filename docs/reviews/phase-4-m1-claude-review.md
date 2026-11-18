# M1 review follow-up · 2026-11-17

Review input: the owner's screenshot of Claude's five points on PR #4.
Baseline: `bfea393`, all six checks passed in
[run 44089677353](https://github.com/dawnwebs/Dawnload/actions/runs/44089677353).
Review fixes were validated separately on `d82bad7`: all six checks passed in
[run 44095865946](https://github.com/dawnwebs/Dawnload/actions/runs/44095865946).

1. **P1: unsafe Windows path components.** Confirmed missing validation in librqbit 9.1 and
   Dawnload's metadata-to-storage boundary. Upstream checks only raw `..`, `/`, `\` and duplicate
   paths. Its filesystem storage pushes the converted path onto the output folder. The portable
   crafted-torrent test fails on the original M1 code: all 22 unsafe cases reach Files/storage;
   it passes with validation before either boundary. A Windows-only, non-writing probe tests
   the exact `C:` prefix escape through upstream parsing and path conversion on both Windows
   runners. Both Windows runners passed the probe and guarded rejection regression, confirming
   native escape semantics and the mitigation without writing outside a directory. See ADR 0013.
   No VM was used.
2. **P2: binary growth and duplicate crypto providers.** Confirmed feature chain:
   `librqbit/rust-tls -> reqwest/rustls -> __rustls-aws-lc-rs`. Dawnload also enables ring.
   Added a provider-neutral feature proposal to the upstream track. Keep the requested published
   9.1 configuration for M1; no fork. The 9.6 MB increase includes BitTorrent and other added
   dependencies, so it cannot all be assigned to AWS-LC without a controlled build comparison.
3. **M2: ratio resets.** Confirmed. `monitor` takes `uploaded_bytes` directly from the live
   handle, while Session reconstruction does not restore a lifetime upload counter. The existing
   ADR already scopes this to M2. Added an explicit SQLite cumulative-counter and near-limit
   pause/restart acceptance test; repeated restarts must not reset the budget or double-count it.
4. **Chooser slots and phase noise.** Both confirmed. The chooser runs in an owned Pulling job,
   and `Actor::busy` excludes only Seeding. Added M3 capacity/reacquisition/cancellation tests to
   the plan. Fixed phase noise in M1: compare both status and detail, emitting only on a change.
   A real no-peer torrent monitor test failed with repeated Pulling events and passes after the
   fix; progress and swarm updates keep their normal cadence.
5. **CI scope.** Intentional. All PR updates, including acceptance evidence, run the five-platform
   matrix and licence gate on their own SHA. Release step wall time and uncompressed binary
   bytes are published in summaries/artifacts. This avoids a docs-only acceptance update having
   no checks on its final head. Runner/cache differences mean timings are not comparable as a
   controlled benchmark. The local review-fix release build is 19,796,656 bytes for the CLI
   and 43,381,728 bytes for the desktop (122.94 s incremental release step). The baseline CI values are recorded in
   [phase-4-m1-ci-bfea393.json](../measurements/phase-4-m1-ci-bfea393.json).

No upstream security report or new upstream PR was sent as part of this review. The TLS proposal
is queued; the path finding should use an agreed upstream reporting route. M2 and M3 work has
not been pulled into M1. PR #4 is unmerged; its source has passed all six checks. Final PR-head
checks must stay green for owner review/merge.

Local review-fix checks: all 86 workspace tests, fmt, clippy with `-D warnings`, and cargo-deny
licences pass. The rebuilt CLI also resumed the already verified Ubuntu ISO through the new
path guard and seeded to a separate local peer (11,309,056 bytes reported uploaded; 9,342,976
bytes verified by the peer). This was a resume/upload smoke test, not a new full ISO download.
Memory figures in the earlier report remain labelled as pre-review-fix measurements.

The five-runner release measurements for the validated review-fix source are in
[phase-4-m1-ci-d82bad7.json](../measurements/phase-4-m1-ci-d82bad7.json). The earlier baseline
measurements remain available for provenance. This closeout only updates acceptance evidence;
it does not change code or dependencies.
