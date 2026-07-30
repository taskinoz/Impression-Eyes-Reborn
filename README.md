# Impression Eyes Reborne
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
- native-size BMP, GIF, ICO, JPEG, PNG, TIFF, and WebP decoding;
- per-pixel image transparency toggled with `F7`;
- always-on-top mode toggled with `F8`;
- click-and-drag movement from anywhere on the image; and
- mouse-wheel navigation through images in the loaded file's folder; and
- `Escape` or `Alt+F4` to close the active viewer.

Animated images currently display their first frame. Startup presentation,
multi-frame animation, context actions, and any additional legacy shortcuts are
still to be documented and implemented.

## Building

Install the Rust toolchain, then run:

```powershell
cargo build --release
```

The executable is written to
`target\release\impression-eyes-reborne.exe`. It can be launched empty for a
drop target or with an image path:

```powershell
.\target\release\impression-eyes-reborne.exe .\picture.png
```

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
