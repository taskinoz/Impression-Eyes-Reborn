# Thumbnail browser source port

Reference: `http://git.local/taskinoz/ime`, main commit
`c421cea85b9aeb7be03b300bab37a65bb9205154`.
Scrolling and worker behaviour were also checked against the local reconstructed
`ime-rebuild` v2 commit `befa31ff0609df83c7a0fd4226156f4cd59edeb5`.
That repository describes itself as a reconstruction from the shipped binary,
not recovered original developer source. This branch implements the relevant
behaviour in Rust; it does not import the MFC runtime, executable resources,
registration system, or binary matching machinery.

## Behaviour brought across

- `EyesThr.cpp`: default seven columns and three rows, 96-pixel cells with
  eight-pixel spacing and a slight four-pixel arc; selected file centred in a
  wrapping folder sequence; centre-out progressive loading.
- `EyesThr.cpp` / `EyeWnd.cpp`: a floating picker anchored near the lower part of
  the viewer and clamped to the current monitor; approximately 1.7× selected preview.
- `docs/USING.md`: Shift hold/hover/release, middle-click toggle, arrows and wheel
  scrolling. The main image changes only when the user commits.
- `EyesThr.cpp::OnGridNavigate`: scroll the folder sequence beneath the selected
  slot instead of recentering it. `EyeWnd.cpp::AnimateStep` supplies three motion
  steps: two midpoint steps and a final snap, on a 30 ms timer. Selection growth
  uses the same motion; reversing mid-scroll starts from the visible position.

## Optimisations

- Ask `IShellItemImageFactory` for Explorer thumbnails instead of fully decoding
  every supported image. Read native dimensions from the image header; retain
  the existing bounded decoder for formats without shell/header support.
- Reuse overlapping loaded cells and maintain an 84-entry LRU thumbnail cache.
  File size and modification time invalidate changed files before cache reuse.
- Cache each cell's resized bitmap. Repainting other cells no longer resamples
  every image, and hovering never rerenders the full-resolution main viewport.
- Use one persistent thumbnail worker, following the original CThumbThr design,
  with one replaceable pending request. Check generations before each file,
  including after errors. An already-running shell handler/decoder cannot be
  interrupted, so cancellation takes effect when that call returns.
- Retain the picker's GDI bitmap, device context and pixel buffer between frames,
  and stop the movement timer when all cells settle.
- Accumulate partial wheel notches and preserve multi-notch input.
- Resolve relative image paths before shell access, matching the v2 OpenFile fix.
- Keep existing 50 ms progressive notification throttling.

## Deliberate implementation differences

The Rust picker uses one transparent owned window instead of one MFC window per
cell. It uses the original default grid, without importing its registry settings
or preferences dialog. Movement animates rectangles within that single window. Escape
cancels the picker. Transparent thumbnail pixels retain their alpha.

## Validation

Unit coverage includes wraparound at both folder boundaries, selection retention,
overlapping cache reuse, early cancellation, decode failure cancellation,
resampling reuse, and alpha compositing, alongside the existing decoder,
navigation, image rendering, and updater suites.

Validated locally: all 39 tests pass; the release build succeeds. Tests also cover
three-step motion, reversing during motion, animated hit testing, small-folder
scrolling, partial wheel input, bounded cache invalidation/reuse, and coalescing
100 queued requests to the latest request.

Desktop checks
with 30 generated numbered images cover middle-click opening/commit, Home and
Right navigation, wheel wraparound, Shift-release commit, Escape cancellation,
and a full picker over a roughly 73×51-pixel main window. These checks exposed
and fixed synthetic mouse-move events overriding keyboard selection after repaint.
The debug linker reports an MSVCRTD/default-library conflict warning;
the debug tests and GUI run successfully and the release build is clean.
The scrolling follow-up additionally verifies fixed-slot navigation, reverse
scrolling, row wraparound, Shift-release commit, and relative-path launch.

Build with the installed `nightly-2026-07-20-x86_64-pc-windows-msvc` toolchain;
the machine's default Rust 1.80 predates APIs already used by this project.
