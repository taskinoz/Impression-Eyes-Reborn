# Selling and publishing ime-reborn on Windows

Last reviewed: 2 August 2026. This is a practical release checklist, not legal advice or a guarantee of Store certification. Recheck the linked Microsoft policies before every major submission.

## Recommended business model

Keep the same full-featured build free on GitHub and sell the Microsoft Store edition for convenience: trusted installation, automatic updates and support for continued development. A starting price around USD 4.99 (or the nearest Store tier) is low-friction and avoids making the open-source version feel intentionally impaired. Keep sponsorship optional and do not promise digital benefits for a donation unless its checkout complies with Store commerce rules.

Use a distinct modern identity such as **ime-reborn**. The old company being gone does not by itself clear its name, artwork or trademarks for commercial use. Before charging money, search IP Australia, USPTO, EUIPO and WIPO databases, retain the clean-room notes, use only original artwork/code, and put a non-affiliation statement in the listing. Microsoft requires apps, names, logos and content to be original, licensed or otherwise permitted by law ([Store policy 11.2](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies#112-content-including-names-logos-original-and-third-party)).

## 1. Create the accounts

1. Create or choose a dedicated Microsoft account with recovery details you control.
2. Enrol in Partner Center as an **individual** Windows developer. Microsoft currently offers individual registration at no charge and requires identity verification using a government ID and selfie ([individual developer onboarding](https://learn.microsoft.com/en-us/windows/apps/publish/whats-new-individual-developer)).
3. Choose the publisher display name carefully. Reserve the app name in Partner Center before printing it into package metadata.
4. Create a dedicated support email. Publish the project website, this privacy policy, source repository and an issue/support route.
5. Complete payout and tax details if you will charge through the Store.

## 2. Finish the release foundations

- Add a complete open-source `LICENSE` file and copyright notices for every bundled dependency and asset.
- Publish the website's `/privacy/` page. A privacy policy is required for Win32 and Desktop Bridge products even when no personal data is collected ([policy 10.5.1](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies#105-personal-information)).
- Document supported formats, Windows versions, shortcuts, accessibility limitations and how to uninstall.
- Add an About/version surface or a documented command that makes the installed version easy to identify.
- Keep release binaries reproducible from tagged source, attach checksums, and retain dependency licence/SBOM records.
- Submit the executable to Microsoft malware analysis if Defender still produces a false positive. Signing and Store distribution improve reputation, but do not bypass security checks.

## 3. Package it as MSIX

MSIX is the best first Store route for this project. The Store signs an accepted MSIX package, so it does not need a public CA code-signing certificate before submission ([publish your first app](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/publish-first-app)). A raw EXE/MSI submission instead needs a CA-signed executable, an immutable versioned HTTPS installer URL, and a silent standalone installer ([policy 10.2.9](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies#102-security)); the current portable EXE is not ready for that route.

1. Install the current Windows SDK and use MSIX Packaging Tool or `MakeAppx.exe` ([MakeAppx guide](https://learn.microsoft.com/en-us/windows/msix/package/create-app-package-with-makeappx-tool)).
2. Package `ime-reborn.exe` and the required visual assets. Do not package build tools or the legacy reference binary.
3. Copy the exact package identity and Publisher values assigned by Partner Center into `AppxManifest.xml`.
4. Declare a desktop/full-trust application and the restricted capability `rescap:Capability Name="runFullTrust"`. Classic medium-integrity Win32 apps require it; Partner Center will request a justification ([capability declarations](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/app-capability-declarations)).
5. Add file-type associations only for formats the release actually opens reliably. Make the verbs descriptive (for example, “Open with ime-reborn”) and test changing defaults through Windows settings rather than trying to force defaults in code.
6. Supply all Store logo/tile assets from the original ime-reborn logo. Never include the old proprietary logo.
7. Verify clean install, upgrade from the previous package, launch by file association, and clean uninstall as a standard user.

Suggested restricted-capability explanation:

> ime-reborn is a traditional Win32 image viewer written in Rust. `runFullTrust` is required to launch its desktop executable at normal user integrity, render a borderless image window, and read only images explicitly opened, dropped, or browsed by the user. It does not elevate, install services, inject or observe input, change system settings, or access the network.

## 4. Certification and test checklist

Run the newest Windows App Certification Kit against the final package, not merely the unpackaged EXE ([WACK documentation](https://learn.microsoft.com/en-us/windows/uwp/debug-test-perf/windows-app-certification-kit)). Its supported-API test inspects imported APIs; fix failures rather than suppressing them.

- Windows 10 and 11, standard user, light/dark themes, mixed DPI and multiple monitors.
- Launch normally, through every registered extension, by drag-and-drop and with an invalid/corrupt file.
- Very large images, animated GIF/WebP, rapid scrolling, zoom, rotation and Shift thumbnail mode.
- Suspend/display changes, Explorer restart, update while closed, uninstall/reinstall.
- No hangs, input backlog, crashes, orphan processes or files left outside permitted app data.
- Screen-reader-visible app/title where practical and keyboard access to essential functions.
- Verify the privacy statement remains true using a network monitor.

## Source-level Store/API audit

The 2 August 2026 source review found no shortcut that violates current Store policy:

| Area | Finding | Store assessment |
| --- | --- | --- |
| Keyboard and wheel | `WM_KEYDOWN`, `WM_KEYUP`, `WM_MOUSEWHEEL`, `GetKeyState`; handled only for the app window | Normal local input. No global hook or interception. |
| Restricted input | No `RegisterHotKey`, keyboard hooks, input injection, input observation or input suppression | None of Microsoft's restricted input capabilities is needed. |
| Window behavior | Borderless/layered window, optional always-on-top, monitor positioning | User-invoked behavior confined to the app's window. |
| File access | Command-line/open association, drag-and-drop, and sibling images in the chosen folder | Appropriate for a full-trust desktop app; do not declare `broadFileSystemAccess`. |
| GDI/Win32 | Documented GDI, message-loop, monitor, timer and Shell drag/drop APIs | No undocumented API was found. Validate the final import table with WACK. |
| Privilege | `asInvoker`, `uiAccess=false`; no service, driver or elevation | Suitable for normal-user execution. Package still needs `runFullTrust`. |

Current shortcuts (Escape, Shift, Ctrl+wheel, R/Ctrl+R, F1, F7, F8, B, Home/End, Space/Backspace and +/-/0) are app-local and do not reserve or replace Windows system shortcuts. Keep future shortcuts local unless there is a compelling, disclosed reason otherwise. Microsoft prohibits undocumented APIs and unsupported techniques, and requires responsive graceful behavior ([policies 10.2 and 10.4](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies)). This review is evidence, not certification; the final MSIX and every native dependency must pass WACK.

## 5. Submit in Partner Center

1. Reserve the distinct product name.
2. Create the submission and complete properties, age ratings, markets, pricing tier and availability ([pricing and availability](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/price-and-availability)).
3. Upload the MSIX/MSIXUPLOAD package.
4. Add polished screenshots, concise features, supported formats, system requirements, support URL, source URL and the public privacy-policy URL.
5. Explain `runFullTrust`, the local folder-browsing behavior and the lack of telemetry in certification notes. Include exact reproduction steps for keyboard features.
6. Submit, monitor the certification report, and answer failures with a new versioned package rather than replacing an already-published download.

Suggested listing language: “An independent, open-source continuation inspired by a discontinued lightweight viewer. Not affiliated with or endorsed by the original publisher.” Have a qualified IP professional review that wording and the chosen name before a paid launch.

## 6. Operate releases

Tag semantic versions (`v0.1.0`, `v0.1.1`), let GitHub Actions build the public artifacts, test the exact hashes, then submit that version to Partner Center. Keep the GitHub and Store binaries functionally identical. Publish release notes and security/contact instructions. Never reuse a package version; increment it for every Store upload.

The Store build should rely exclusively on Microsoft Store updates. The portable and installed GitHub distribution strategy, including the separate non-resident updater, is documented in [`UPDATES.md`](UPDATES.md).

Review Store policies before each submission because requirements change. In particular, keep capabilities minimal and tied to functionality ([policy 10.6](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies#106-capabilities)), keep the listing accurate, and make sure the app starts promptly, remains responsive and shuts down gracefully.
