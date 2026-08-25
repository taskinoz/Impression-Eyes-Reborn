# Impression Eyes Reborn
A new open-source, clean-room image viewer written in Rust and inspired by the
user experience of Impression Eyes.

## Project Status

The original application has been preserved locally as a compatibility
reference, but it is excluded from this repository. The replacement will be
implemented from public documentation and observable behavior rather than from
decompiled or disassembled code.

The locally preserved `ime.exe` identifies itself as Impression Eyes 1.1.2.6.
Both preserved copies are byte-identical (MD5
`df2b066f74c17efd8db85d7178f73cdf`).

## Current Prototype

The first Windows prototype implements the core overlay workflow:

- a tiny, borderless native Win32 window;
- images opened from the command line or dropped onto the window;
- native-size AVIF, BMP, DDS, Farbfeld, GIF, ICO, JPEG, JPEG XL, PNG, PNM,
  QOI, TGA, TIFF, and WebP decoding;
- timed animated PNG (APNG), GIF, and WebP playback;
- per-pixel image transparency toggled with `F7`;
- always-on-top mode toggled with `F8`;
- click-and-drag movement from anywhere on the image; and
- edge/corner resizing with smooth image scaling;
- mouse-wheel navigation through images in the loaded file's folder;
- zoom from 10% to 800% with `Ctrl` + mouse wheel or `+` / `-`, with `0`
  restoring the fitted 100% view without changing the window size; and
- clockwise rotation with `R` and counter-clockwise rotation with `Ctrl+R`;
- `Escape` or `Alt+F4` to close the active viewer.

Folder navigation follows Explorer-style natural filename ordering (`image2`
before `image10`). `Home` selects the first image, `End` the last, `Space`
or Right Arrow selects the next image, and `Backspace` or Left Arrow selects
the previous image; wheel and relative-key navigation wrap at folder
boundaries.

Press `Delete` to move the current image to the Windows Recycle Bin and open
the next image in the folder. Deleting the final image closes the viewer.

The empty startup window shows the drop target and advertises `F1`. Press `F1`
at any time to display the complete shortcut reference.

The temporary filename label also shows the current zoom percentage. Zooming
scales and centre-crops the image inside the existing viewport. It resets to
the fitted 100% view when moving to another image.

Rotation happens around the viewer centre and refits the rotated dimensions to
the active monitor. The thumbnail selector remains upright and is never
rotated with the main image.

On launch, the viewer is centered in the usable area of the monitor containing
the previously focused window. After launch, moving or navigating the viewer
does not reset the position.

Images keep their aspect ratio during normal resizing; hold `Ctrl` while
resizing for freeform stretching. Hold `Shift` to open a centered thumbnail
grid, hover an image to enlarge/select it, and release `Shift` to load it.
Full-resolution images and thumbnails decode on independent background workers.
The grid appears immediately and fills progressively, prioritizing smaller,
faster files so a single large image does not hold up the entire selector.
Changing images restores that image's native
dimensions unless it exceeds the active monitor's usable area, in which case it
is scaled down proportionally. The filename is displayed briefly. Press `B` to
cycle black, white, and checkerboard transparency backgrounds, or `F7` for true
desktop transparency.

Animation frame timing is bounded to a safe minimum and decoded frame memory is
limited. Context actions and additional legacy shortcuts remain to be
implemented.

## Building

Install the Rust toolchain, then run:

```powershell
cargo build --release
```

The executable is written to `target\release\ime-reborn.exe`. It can be launched empty for a
drop target or with an image path:

```powershell
.\target\release\ime-reborn.exe .\picture.png
```

### Microsoft Store package

Reserve the app in Partner Center, then copy the exact values shown under
**Product management → Product identity** into this command:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\packaging\msix\build-msix.ps1 `
  -IdentityName "YourStoreIdentityName" `
  -Publisher "CN=your-partner-center-publisher-id" `
  -PublisherDisplayName "Your publisher display name"
```

The script builds both an unsigned `.msix` and the recommended
`.msixupload` submission archive in `dist`. Partner Center signs the
accepted Store package. The raw MSIX is intentionally unsigned; local
sideloading requires a certificate whose subject matches `Publisher`.

Tagged GitHub releases build these packages automatically when the repository
variables `MSIX_IDENTITY_NAME`, `MSIX_PUBLISHER`, and
`MSIX_PUBLISHER_DISPLAY_NAME` are configured. These values are assigned by
Partner Center and must match exactly.

## History
The original developer has passed away and the closed-source application is no
longer maintained. This project aims to provide a maintainable replacement; it
does not include or redistribute the original program.

## Archive Downloads
https://impression-eyes.software.informer.com/
https://en.freedownloadmanager.org/Windows-PC/Impression-Eyes.html
https://impression-eyes.software.informer.com/download/
https://www.ffxiah.com/forum/topic/30669/valeths-custom-health-meters/
https://polycount.com/discussion/55102/become-more-efficient-free-or-nearly-free-app-039-s-websites-to-help-us-do-our-jobs/p2
