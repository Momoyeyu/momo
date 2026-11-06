# Electron 017 macOS candidate — 2026-11-06

Status: implemented local macOS arm64 candidate; product acceptance pending. Windows follows macOS acceptance. This is not a notarized release.

## Source and reproducibility

Shared business source: `fe3f7cc253603b5c4c21bafd6d0fc1ac709a4cd2` from momo-web. [Exact-source CI](https://github.com/Momoyeyu/momo-web/actions/runs/42088743907) passed. `app/migration/web-source.json` contains all 811 promoted file hashes; the same manifest is included in app.asar. No business implementation was forked into Electron.

Electron 45.2.3 / Forge 8.0.1 / TypeScript 6.0.2 / pnpm 12.6.0. Offline plugin Node is pinned separately to 24.11.0. The package contains Python, ASR/embedding, Live2D/Core, Cordis, native helper, Minecraft companion JAR and isolated QQ resource closure. No Docker or development tools are required by the installed runtime.

## Verified

| Layer | Evidence |
| --- | --- |
| Shared backend | 737 passed, 3 skipped |
| Shared frontend | 247 passed; typecheck and production build passed |
| Plugin host | 296 passed, 1 skipped |
| Electron shell | 15 passed; strict TypeScript and Prettier checks |
| Frozen backend | Real E5 embedding/sqlite-vec, SenseVoice/VAD, Core, Unicode paths, desktop origin/token enforcement and EOF shutdown |
| Installed application | DMG mounted and copied into a separate local installation; fresh profile, restart, second instance, intro entry, Boot/chat/Live2D/settings/Home, synthetic SSE model reply, unauthenticated/untrusted-origin denial, QQ installed but dormant, exit reclaims backend |
| Backup | UI-created ZIP contains SQLite memory and exact IndexedDB Blob bytes; deletion followed by UI import restores both |
| Resources | Python and QQ framework relative symlinks resolve after relocation; source revision extracted from packaged asar matches promotion manifest |
| Visual inspection | Stable intro, chat and settings screenshots inspected after transition completion |
| Candidate sealing | Forge ad-hoc signing; `codesign --verify --deep --strict` passed on packaged bundle; no Developer ID or notarization |

The native test creates and deletes its own profile. Its model is a local synthetic HTTP provider, not a paid or live user API. QQ remains disconnected. Backup export captures the generated Blob to avoid driving the system Save As dialog; the subsequent import is the real UI flow. No real user's configuration, credentials, memories or accounts were modified.

## Measurements and limits

Measured on MacBook Air `Mac16,12`, 24 GiB RAM. Startup is process launch → title DOM visible, not a formal interactive-ready benchmark. CPU/RSS samples aggregate the owned process tree using `ps`, three samples per scene one second apart; they include active animation/startup and are not steady-state measurements. The initial raw `.workbuddy/017/evidence/` and installer were removed during user-authorized storage cleanup. Historical results above are summaries, not currently available raw attachments.

Real microphone/recording/accessibility system prompts and denial recovery, sleep/resume, real QQ messaging, Minecraft/application control, full audio/media/i18n regression and a clean machine without developer tools remain unverified in this migration. Third-party redistribution qualification and Developer ID signing/notarization remain release work. Windows process-tree behavior is not yet implemented/validated as a supported platform.

## Review corrections

- Restored trusted-main-frame display selection while keeping external frames/camera denied.
- Added Minecraft companion JAR to package preparation and resolved its frozen path in shared source.
- Set Swift helper deployment target to macOS 14.
- Added bounded, redacted startup diagnostics and latched unexpected backend failure to prevent automatic respawn.
- Normalized Forge-copied absolute symlinks in two passes, validated complete chains, and used one resource list including QQ.
- Retained the native Edit menu for clipboard shortcuts; removed Tauri/WDIO/duplicate packaging paths.

Heavy builds/model tests ran in the task-owned Mac Studio directory. Its Swift CLT module conflict required only the small helper to be compiled locally with an explicit macOS 14 target. Artifacts and logs are collected before remote cleanup. The old tauri-consolidation checkout was later removed with explicit user authorization after verifying its source exists in remote Git history.


## macOS experience follow-up — 2026-11-06

The user deferred Developer ID signing/notarization and formal distribution; these are not acceptance blockers for the local experience candidate. Windows still follows macOS acceptance.

Native dialogs now follow the existing authenticated `settings.general.lang` (zh/en/ja), with OS language fallback before backend readiness and cached language when unavailable. The lookup is bounded to 750 ms and introduces no renderer bridge or alternate settings store. English diagnostic logging remains separate from translated UI.

18 shell tests passed, including current UI language changes, authentication, fallback and timeout. Forge packaged the updated shell on Mac Studio; packaged frontend/migration and all 4,702 runtime files/links match the previous installation. Strict/deep ad-hoc signature verification passed. Remote task artifacts were removed after transfer.

Manual installed-app checks: microphone entered listening and stopped successfully; desktop plugin discovered existing windows after enabling. This does not establish real transcription accuracy, screen capture/input authorization, denial recovery or sleep/resume. No application control was performed and the desktop plugin was restored to disabled.

Updated candidate native smoke passed (isolated profile, synthetic model, backup SQLite/IndexedDB round trip, lifecycle and auth). The initial run exceeded the previous 15 s reload wait; rerun with a 45 s bound passed. Failure screenshot/text diagnostics were added; no product workaround or relaxed data assertions. Updated app installed at `/Applications/momo.app`; old app copy removed.


QQ real-account follow-up: the user scanned the installed app's QR code. Login was verified, receive/reply permissions were enabled, and incoming private text received a delivered reply identifying QQ correctly. The phone automatically opened and showed the latest conversation. No contact/history/media/group permissions were enabled for this text-only test. The transient untranslated preparing status was fixed upstream in 7e190f2 and promoted through the manifest; shared CI passed.
