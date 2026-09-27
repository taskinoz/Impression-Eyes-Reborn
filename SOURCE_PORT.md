# Thumbnail browser source port

Reference: `http://git.local/taskinoz/ime`, main commit
`c421cea85b9aeb7be03b300bab37a65bb9205154`.
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

## Optimisations

- Ask `IShellItemImageFactory` for Explorer thumbnails instead of fully decoding
  every supported image. Read native dimensions from the image header; retain
  the existing bounded decoder for formats without shell/header support.
- Reuse overlapping loaded cells when paging; retain at most 21 cells per current
  snapshot rather than building an unbounded folder cache.
- Cache each cell's resized bitmap. Repainting other cells no longer resamples
  every image, and hovering never rerenders the full-resolution main viewport.
- Serialise thumbnail decode work separately from full-image decoding and check
  generations before each file, including after errors. Superseded queued jobs
  leave without decoding. An already-running shell handler/decoder cannot be
  interrupted, so cancellation takes effect when that call returns.
- Keep existing 50 ms progressive notification throttling.

## Deliberate implementation differences

The Rust picker uses one transparent owned window instead of one MFC window per
cell. It uses the original default grid, without importing its registry settings
or preferences dialog. Preview enlargement and scrolling update immediately;
the original three-tick window movement animation is not reproduced. Escape
cancels the picker. Transparent thumbnail pixels retain their alpha.

## Validation

Unit coverage includes wraparound at both folder boundaries, selection retention,
overlapping cache reuse, early cancellation, decode failure cancellation,
resampling reuse, and alpha compositing, alongside the existing decoder,
navigation, image rendering, and updater suites.

Validated locally: all 31 tests pass; the release build succeeds. Desktop checks
with 30 generated numbered images cover middle-click opening/commit, Home and
Right navigation, wheel wraparound, Shift-release commit, Escape cancellation,
and a full picker over a roughly 73×51-pixel main window. These checks exposed
and fixed synthetic mouse-move events overriding keyboard selection after repaint.
The debug linker reports an MSVCRTD/default-library conflict warning;
the debug tests and GUI run successfully and the release build is clean.

Build with the installed `nightly-2026-07-20-x86_64-pc-windows-msvc` toolchain;
the machine's default Rust 1.80 predates APIs already used by this project.
