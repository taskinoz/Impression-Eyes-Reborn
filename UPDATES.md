# Update strategy

The viewer should remain a small, offline executable. Update discovery, downloads and installation belong to a separate optional component.

## Release channels

| Channel | Package | Update mechanism |
| --- | --- | --- |
| Microsoft Store | MSIX | Microsoft Store updates only |
| Portable | Versioned ZIP containing `ime-reborn.exe` | No automatic updater; replace the folder manually |
| Installed | Per-user installer | Separate `ime-reborn-updater.exe` scheduled task |

All three channels should be built from the same signed tag and contain the same viewer functionality. Store installations must not install or run the GitHub updater.

## Recommended installed layout

Install per user so setup and updates do not require elevation:

```text
%LOCALAPPDATA%\Programs\ime-reborn\
  ime-reborn.exe
  ime-reborn-updater.exe
  uninstall.exe
  current-version.txt
```

The installer creates Start menu shortcuts, file associations and an **optional, clearly labelled** scheduled task. In NSIS this is a default-off component named **Automatic update checks**. Selecting it installs both the updater and its task; uninstalling removes both.

The normal viewer component remains mandatory. The initial release supports interactive component selection; a documented command-line component selector can be added if enterprise deployment requires unattended opt-in choices.

Do not use a Windows service. A delayed scheduled task is simpler, consumes no memory between checks and does not require administrative privileges:

- Trigger at user logon with a 5-minute delay.
- Repeat approximately every 24 hours.
- Start only when a network connection is available.
- Stop if it runs for more than 2 minutes.
- Run only as the signed-in user and never request elevation.

The updater should normally have no tray icon or resident process. An “Check for updates” Start menu shortcut can run `ime-reborn-updater.exe --interactive` and show a small result dialog.

## GitHub release contract

Every tagged GitHub release should publish:

```text
ime-reborn-v0.2.0-windows-x86_64.zip       portable package
ime-reborn-v0.2.0-windows-x86_64-setup.exe installed package
ime-reborn-update.json                     updater manifest
SHA256SUMS.txt
SHA256SUMS.txt.minisig                      signature
```

The updater can fetch one stable URL without using the rate-limited GitHub API:

```text
https://github.com/taskinoz/Impression-Eyes-Reborn/releases/latest/download/ime-reborn-update.json
```

Example manifest:

```json
{
  "schema": 1,
  "version": "0.2.0",
  "published": "2026-08-02T12:00:00Z",
  "minimum_updater": "0.1.0",
  "installer_url": "https://github.com/taskinoz/Impression-Eyes-Reborn/releases/download/v0.2.0/ime-reborn-v0.2.0-windows-x86_64-setup.exe",
  "installer_sha256": "hex digest here",
  "notes_url": "https://github.com/taskinoz/Impression-Eyes-Reborn/releases/tag/v0.2.0"
}
```

The manifest must contain an immutable, versioned asset URL. Do not place a mutable `latest` URL in `installer_url`. Generate the manifest and hashes in the release workflow so the version, filenames and digest cannot drift apart.

## Updater behaviour

Each scheduled run should:

1. Acquire a named mutex so only one updater instance runs.
2. Read the installed version locally. Parse versions with a tested semantic-version library; never compare version strings lexicographically.
3. Fetch `ime-reborn-update.json` over HTTPS with short connection and total timeouts. Send a descriptive user agent and accept redirects only to HTTPS.
4. Validate the manifest schema, maximum size, expected GitHub repository/host, version and URL. Ignore an equal or older version.
5. Download to a unique staging directory under `%LOCALAPPDATA%\ime-reborn\updates\`. Apply strict size limits and stream to disk rather than buffering the installer in memory.
6. Verify SHA-256 and then verify the release signature or Authenticode signature. A checksum fetched from the same compromised location detects corruption but is not, by itself, proof of publisher identity.
7. Check whether `ime-reborn.exe` is running. Never terminate it. If open, retain or discard the staged update and try again on the next scheduled run.
8. Launch the verified installer in its documented silent-update mode. The installer performs replacement and rollback; the updater must not overwrite its own running executable.
9. Write a small bounded log, remove stale staging files and exit.

Updates should be quiet when successful. In interactive mode, display “Up to date,” the available version, release-notes link, validation failures or network errors. Scheduled network failures should not interrupt the user.

## Installer responsibilities

Use the repository's NSIS installer rather than writing installation and uninstall bookkeeping in Rust. It must support:

- New per-user installation and silent upgrade.
- Atomic-enough replacement with rollback if installation fails.
- Version downgrade prevention except through an explicit developer recovery command.
- File associations without forcibly taking defaults away from another viewer.
- Optional scheduled-task installation.
- Clean uninstall and removal of the scheduled task.
- Authenticode signing when a signing certificate becomes available.

The updater should download the complete installer initially. Delta updates add complexity, more failure modes and little benefit for an application this small.

## Authenticity and compromise resistance

Before unsigned community previews, HTTPS plus a pinned repository and SHA-256 provides corruption protection but limited publisher authentication. For a paid/public release:

1. Authenticode-sign the viewer, updater and installer with the same publisher identity.
2. Verify the downloaded installer's signature and expected signer in the updater before execution.
3. Sign `SHA256SUMS.txt` with an offline minisign key and embed only its public key in the updater.
4. Protect GitHub releases with tag protection, required CI checks, least-privilege workflow permissions and protected signing secrets.
5. Never execute arbitrary commands, filenames or URLs supplied by release notes or untrusted manifest fields.

Signing the manifest does not replace signing executables; use both when practical.

## Versioning and compatibility

- Use semantic Git tags such as `v0.2.0`; package metadata uses `0.2.0`.
- Stable installed builds consume only stable, non-prerelease GitHub Releases.
- Preview builds use a separate opt-in channel and manifest.
- An updater may update itself only through the installer.
- `minimum_updater` lets a release decline an unsafe upgrade and direct the user to the website.
- Keep the last working installer available for support, but do not let the automatic channel downgrade users.

## Privacy

The portable and Store builds make no update requests. When enabled, the installed updater sends a routine HTTPS request to GitHub on its schedule. The website privacy policy and installer disclosure must say this. Do not send installation IDs, image names, paths, usage metrics or device identifiers. GitHub will necessarily receive ordinary connection information such as the IP address and user agent.

## Delivery phases

### Phase 1 — next release

- Continue publishing the portable ZIP and raw executable.
- Add immutable filenames and a generated update manifest to the release workflow.
- Add an NSIS per-user installer with a mandatory viewer component and an unselected automatic-updates component. Until the updater implementation is release-ready, omit the optional component from public installers rather than installing a placeholder task.
- Submit the installer manifest to Windows Package Manager (`winget`) as another manual update route.

### Phase 2 — optional updater

- Build `ime-reborn-updater` as a separate small Rust binary with no image-decoding dependencies.
- Add manifest parsing, bounded HTTPS download, semantic version comparison, SHA-256/signature verification, mutex and staged installer execution.
- Add the opt-in scheduled task to the installer.
- Verify install, maintenance-mode deselection, repair, major upgrade and uninstall all leave exactly the expected task state.
- Test offline, proxy, redirect, truncated download, bad hash/signature, disk-full, viewer-open, concurrent-run, upgrade, rollback and uninstall scenarios.

### Phase 3 — signed public release

- Acquire an appropriate code-signing arrangement.
- Sign every Windows artifact and enforce signer validation.
- Enable the scheduled updater by default with clear installer disclosure.
- Publish the Store MSIX without the updater and verify that the channels cannot be accidentally mixed.

## Acceptance criteria

- `ime-reborn.exe` contains no networking or updater code.
- No updater process remains running after a check.
- Portable and Store users never receive the scheduled task.
- The updater cannot install an older, corrupted, unsigned or unexpectedly hosted payload.
- Open viewer windows are never killed for an update.
- A failed update leaves the existing viewer launchable.
- Uninstall leaves no scheduled task or update cache behind.

## Installer choice

The GitHub installed edition uses NSIS. Its permissive licensing, small standalone output and component page suit this lightweight open-source application. Keep the NSIS licence notice with project compliance records and review the licences of any third-party NSIS plugins before adding them.
