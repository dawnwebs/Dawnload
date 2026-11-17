# 0013 · Validate torrent path components before storage

Status: Accepted (2026-11-17), M1 review fix.

## Context

librqbit 9.1 rejects `..`, slash and backslash in torrent path components. It does not reject
Windows drive prefixes, NTFS alternate-stream names or DOS device names. Its filesystem backend
builds a path by pushing the metadata path onto the output folder. Rust documents that pushing
a drive prefix without a root replaces the destination on Windows. The CLI already accepts
untrusted torrents in M1; desktop gating until M3 does not protect it.

## Decision

After `list_only` resolves metadata, validate every decoded file path component before emitting
`Files`, persisting metadata, or adding a managed torrent that can create payload files. This
single boundary covers local files, URLs, magnets and metadata recovered from SQLite.

Apply the same portable rules on every OS: reject empty, `.` and `..` components; Windows
reserved characters and ASCII controls; trailing dots/spaces; and DOS device names including
extensions, console device names, and COM/LPT superscript-digit aliases. Reject rather than
rename, since renaming could alias another torrent entry. Normal Unicode and padding paths
remain valid. The destination itself remains the folder the person chose.

This is lexical metadata validation, not a filesystem sandbox against pre-existing symlinks or
junctions. It neither changes librqbit nor adds a Dawnload fork. Prepare the upstream finding
for responsible reporting; do not silently publish a third-party security report.

## Alternatives

Waiting for M3 leaves the CLI exposed. Depending on file-open errors happens too late and does
not prevent drive-relative writes. Platform-only checks leave unsafe metadata portable between
installations; portable rejection gives consistent behavior. Sanitizing names risks collisions.

## Consequences

- A generated-torrent regression covers 22 single/multi-file unsafe-name cases. On the original
  M1 code all 22 reached Files/storage; after validation they fail before either boundary.
- A Windows-only test parses a crafted `C:` torrent through upstream validation and proves
  its converted path does not stay under the chosen destination. It opens no file. Both
  Windows CI runners must pass this probe and the guarded rejection test before M1 is ready.
- The path rules can reject POSIX-valid names that cannot safely travel to Windows. This is a
  deliberate cross-platform constraint; it does not claim all filesystem aliasing is solved.
- Sources: [Rust PathBuf::push](https://doc.rust-lang.org/std/path/struct.PathBuf.html#method.push),
  [Windows file naming](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file).
