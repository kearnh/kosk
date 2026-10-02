# Public alpha distribution

Status: local ZIP built and verified; not published. First-launch provisioning is implemented.
Reviewed 2026-10-03. KOSK code uses MIT; bundled data and artwork retain separate terms.

## Release contract

- Windows x64 ZIP: `kosk-0.1.0-alpha.1-windows-x64.zip`.
- Public download through [GitHub Releases](https://github.com/kearnh/kosk/releases).
- Unsigned alpha. No signing service or certificate setup.
- Extract the whole folder and run `kosk.exe`. No Rust, terminal, administrator
  rights, installer, or first-launch download.
- KOSK starts hidden. The included quick-start explains Ctrl+Alt+F10 and the tray icon.
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
working directory. The Factorio theme uses colours only.

A checksum detects changed files; it does not establish publisher identity.
The package uses a static C runtime. Local PE inspection found no separate
C runtime imports. Verify it on Windows without Rust or Visual Studio.
Package only dependencies actually required and redistributable.

## First-launch and update behavior

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
6. Valid packaged models suppress the developer-oriented next-word setup notice.
   Missing bundled model files stop provisioning rather than activating partial data.
7. Tests cover fresh setup, repeated launches, upgrades, edited custom assets,
   custom model paths, interrupted provisioning, and unwritable directories.

The extracted ZIP starts with Glow and Wgpu using isolated user data and an
unrelated working directory. Installed assets match their recorded hashes;
custom settings and themes survive relaunch. Explicit-config launch leaves
the user data directory untouched. This is local startup verification, not
clean-machine or controller interaction verification.

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

Controller glyphs use Kenney Input Prompts 1.5A under CC0. Include
[the glyph notice](../../assets/controller-glyphs/README.md) and its upstream
license. Audit dependency licenses/notices, including embedded fonts and icons.

## Hosting and Windows security prompts

Use a public release page with versioned assets, source revision, changes,
known limitations, checksums, and support/reporting links. The repository is
[kearnh/kosk](https://github.com/kearnh/kosk); `origin` uses its HTTPS URL.
Finish the user README before the first source push.

The alpha is unsigned. State this in the README and release notes.
[SmartScreen](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)
may warn; Smart App Control or enterprise policy may block execution.
Do not instruct users to disable Windows protection.

## Release gate

- [x] First-launch provisioning and update preservation implemented and tested.
- [ ] Completion inputs cleared; nonempty contextual tables rebuilt and verified.
- [ ] Asset and dependency license notices complete.
- [x] Repeatable Windows x64 packaging with manifest and checksums.
- [x] Unsigned alpha selected.
- [ ] Downloaded ZIP tested on clean Windows, offline and as a standard user.
- [ ] Ctrl+Alt+F10/tray visibility, controller typing, themes, prefix and contextual
      suggestions, upgrade, and removal verified from the extracted package.
- [x] Public repository and GitHub Releases destination selected.
- [ ] Bug-reporting channel documented.
- [ ] User README finalized against verified behavior.

Publish only after these gates pass. The local ZIP includes themes and contextual
completion tables; public asset redistribution checks remain unresolved.
