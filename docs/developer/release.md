# Packaging the alpha

Run from a clean, committed Jujutsu working copy using PowerShell 7:

```powershell
./scripts/package-alpha.ps1
```

The script builds Windows x64 with both renderers, rebuilds completion tables
from `data/completion/en/unigrams.tsv` and the local `count_2w.txt`, checks
contextual predictions, and packages an explicit file list. Supply another
pair-count input with `-BigramSource PATH`. Empty optional trigram tables are
omitted. Outputs are `target/dist/*.zip` and adjacent SHA-256 checksums.

The ZIP includes source revision, input hashes, asset hashes, toolchain, build
command, and dependency notices. Build metadata uses `jj`; the release script
supplies the committed source revision explicitly. Signing is not required.

On ordinary launch, `resources/bundle.toml` beside the executable identifies
the asset generation. KOSK installs it atomically under
`%LOCALAPPDATA%\kosk\bundled\<content-id>` and activates it through
`bundled/current.toml`. Missing bundles preserve development behavior.
Explicit configuration paths do not provision or load these release defaults.

Bundled model and theme paths form a default layer below sparse user settings.
Saving unrelated settings does not pin a particular generation. Custom themes
in `themes/*.toml` are discovered by default; a supplied `themes` list replaces
the default list. Duplicate names retain the existing invalid-theme reporting.
User files and previous asset generations are preserved during updates.

Before public publication, resolve the pending checks in `BUILD-MANIFEST.json`
and verify the downloaded ZIP on clean Windows. Local package verification
does not establish compatibility with a machine without development tools.
