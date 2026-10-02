# Public alpha distribution

Status: release contract and implementation checklist; no downloadable release exists.
Reviewed 2026-10-02. KOSK code uses MIT; bundled data and artwork retain separate terms.

## Release contract

- Windows x64 ZIP: `kosk-0.1.0-alpha.1-windows-x64.zip`.
- Extract the whole folder and run `kosk.exe`. No Rust, terminal, administrator
  rights, installer, or first-launch download.
- KOSK starts hidden. The included quick-start explains F3 and the tray icon.
- Default layouts, mappings, themes, and English suggestions work offline.
- Settings and learned words remain under `%LOCALAPPDATA%\kosk`.
- Updates: quit KOSK, extract the new release into a new folder, run it.
  Preserve settings, custom themes/layouts/bindings, and learned words.
- Removal: quit and delete the application folder. Delete
  `%LOCALAPPDATA%\kosk` only to remove settings and learned words as well.
- No automatic startup or updater for the alpha.
- Target Windows 11 x64 initially. Claim support only after clean-machine
  verification; do not advertise untested Windows versions or controllers.

## Package contents

An allowlist must select `kosk.exe`, approved bundled themes, prepared English
completion tables, a short quick-start, MIT license, and third-party notices.
Publish a SHA-256 checksum alongside the ZIP. Record the source commit,
toolchain, model-input hashes, and build commands in a release manifest.

Exclude development binaries, recordings, captures, personal completion
caches, private files, raw corpora, and repository metadata. Never ZIP the
working directory. Do not ship Factorio images without redistribution terms;
the palette-only Factorio theme currently has its image settings commented out.

A checksum detects changed files; it does not establish publisher identity.
Verify DLL/runtime requirements on a Windows installation without Rust or
Visual Studio. Package only dependencies actually required and redistributable.

## First-launch and update implementation

1. Provision bundled assets before loading default user configuration.
   Existing per-user settings creation is already implemented.
2. Keep release-owned assets in a versioned `bundled/` directory under the
   user data directory. Keep user-editable assets separately. Update bundled
   defaults without overwriting user files.
3. Discover bundled themes automatically. Users can copy one into their
   custom theme directory to edit it; define how duplicate names are handled.
4. Resolve the bundled completion model independently of the working
   directory. Preserve an explicit user model path. Provision complete models
   atomically; an interrupted write must not leave a partially usable model.
5. Keep explicit-config launches isolated: no asset extraction or writes
   outside the supplied configuration's existing behavior.
6. Replace the developer-oriented next-word setup notice for packaged builds.
   Missing bundled data should produce a repair instruction, not a Cargo recipe.
7. Exercise fresh setup, repeated launches, upgrades, edited custom assets,
   custom model paths, interrupted provisioning, and unwritable directories.

## Completion data and credits

The local packed vocabulary, unigrams, and bigrams total about 3.5 MB
uncompressed. Bundling is practical; users need not download or build a model.
Rebuild from pinned inputs rather than copying unverified ignored outputs.
Fail packaging when contextual bigrams are absent; an empty table can still
be produced successfully by the existing builder.

The embedded English word list derives from FrequencyWords' CC BY-SA 4.0
content. Its attribution and modifications are recorded in
[the data notice](../../data/completion/en/README.md). Include that notice
with the binary even when the word list is embedded.

Norvig's [source page](https://www.norvig.com/ngrams/) explicitly licenses
code, but leaves data redistribution terms unclear. Confirm the applicable
terms or build from a corpus with explicit redistribution permission before
shipping next-word tables. Download availability alone is insufficient evidence.

Audit controller glyph provenance and the licenses/notices of dependencies,
including embedded fonts and icons. The two Factorio images have ownership
notes in [themes/images/README.md](../../themes/images/README.md), but no
recorded redistribution grant. Exclude them from the alpha package.

## Signing and hosting

Use a public release page with versioned assets, source revision, changes,
known limitations, checksums, and support/reporting links. GitHub Releases is
the proposed host; `jj git remote list` currently returns no remotes.
Repository ownership and destination must be established before publishing.

Investigate [SignPath Foundation](https://signpath.org/terms.html) for free
open-source signing. Acceptance is not guaranteed; it requires a public,
documented, maintained project, verifiable builds, signing policy, and release
approval. Do not claim sponsorship before acceptance.

[Microsoft's signing guidance](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)
currently limits Artifact Signing for individuals to the USA and Canada.
Check publisher eligibility before selecting a paid service. Sign and timestamp
the executable before packaging; then compute checksums.

An unsigned public alpha is possible, but the release page must state that it
is unsigned. [SmartScreen](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)
can warn for new signed builds too, and Smart App Control or enterprise policy
can block unsigned builds. Do not instruct users to disable Windows protection.
An installer does not solve these signing or reputation issues.

## Release gate

- [ ] First-launch provisioning and update preservation implemented and tested.
- [ ] Completion inputs cleared; nonempty contextual tables rebuilt and verified.
- [ ] Asset and dependency license notices complete.
- [ ] Repeatable Windows x64 packaging with manifest and checksums.
- [ ] Signing arranged, or unsigned-release limitations explicitly accepted.
- [ ] Downloaded ZIP tested on clean Windows, offline and as a standard user.
- [ ] F3/tray visibility, controller typing, themes, prefix and contextual
      suggestions, upgrade, and removal verified from the extracted package.
- [ ] Public repository, release destination, and bug-reporting channel selected.
- [ ] User README finalized against verified behavior.

Publish only after these gates pass. The current executable is unsigned;
settings/layouts/mappings/prefix word-list fallback already work without
repository files, but theme and next-word provisioning remain unimplemented.
