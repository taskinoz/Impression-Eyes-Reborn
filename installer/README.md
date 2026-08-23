# NSIS installer

The GitHub installed edition uses NSIS and installs per user without elevation. The viewer is required. Automatic updates and all file-association families are optional and unselected by default.

Related extensions are grouped: JPEG registers `.jpg` and `.jpeg`, TIFF registers `.tif` and `.tiff`, and PNM registers `.pnm`, `.pbm`, `.pgm`, and `.ppm`. Registration adds ime-reborn to Windows **Open with** choices; it does not forcibly replace the user's current default application.

## Build

Install current NSIS, build the Rust release, then run:

```powershell
cargo build --release --locked
New-Item -ItemType Directory -Force dist | Out-Null
makensis.exe /DVERSION=0.1.0 /DVIEWER_EXE="$PWD\target\release\ime-reborn.exe" installer\ime-reborn.nsi
```

The output is `dist\ime-reborn-v0.1.0-windows-x86_64-setup.exe`.

Include the default-off update option with:

```powershell
makensis.exe /DVERSION=0.1.0 `
  /DVIEWER_EXE="$PWD\target\release\ime-reborn.exe" `
  /DUPDATER_EXE="$PWD\target\release\ime-reborn-updater.exe" `
  installer\ime-reborn.nsi
```

The updater implements `--install-task`, `--remove-task`, `--scheduled`, and `--interactive`. The installer hides the update component when that executable is not supplied, which keeps portable and Store packaging updater-free.

## Silent installation

NSIS supports `/S` for silent installation. Optional component selection can be automated later with a documented command-line selection helper if enterprise deployment becomes a requirement; the interactive installer is the supported path for choosing associations in the initial release.

## Release checks

Test fresh install, upgrade while closed, refusal while the viewer is open, every association family, Windows Default Apps/Open with behaviour, silent uninstall, normal uninstall, and complete scheduled-task cleanup. Authenticode-sign the final installer before public distribution.
