# Electron macOS experience verification

## Current acceptance — 2026-11-13

This section supersedes the historical pending entries below. Current shared business revision: `21529eb`; Electron shell revision: `992b143`. The local development certificate and designated requirement remain stable across the installed update; no TCC reset was used for this batch.

| Plan | Actual verification | Status |
| --- | --- | --- |
| 006 | Installed pet drag moved (-45,-40) logical pixels; full/half body, collapse/expand, click-through with usable toolbar, same renderer/lease and persisted preferences; real model screen reading | Accepted macOS experience scope |
| 008 | Existing Chrome form filled/submitted by model; existing Safari focused, input clicked, SafariMouseOK typed, Apply clicked and result read by model; independently confirmed rendered page | Accepted macOS experience scope |
| 007 | Existing Fabric 1.21.1 player/world, no new account/client. Actual model menu/movement/mining/pickup, 2 logs → 8 planks → 1 crafting table + 4 planks, and chat stop verified with bounded instructions. Combat/eating and long-goal autonomy remain unverified | Open |
| 017 | Current stable-signed bundle installed; actual frozen E5/sqlite-vec and SenseVoice/VAD; isolated installed lifecycle, intro, single-instance, pet preferences, chat/settings/Home, SQLite + IndexedDB backup round trip, exit cleanup | Accepted macOS experience scope |

The backend suite passed **824 tests, 3 skipped**. Previous unchanged frontend suite passed 255 tests; plugin host 301 passed / 1 skipped; shell 36 passed. Both repositories' last installed predecessors passed CI; current shared and desktop revisions passed CI. Passing mocks does not substitute for the real browser/game observations above.

Corrections from actual operation: pointer coordinates on every drag event; measured provider prefix budget and post-receipt image fitting; focused AX window title/geometry identity; real AX hit-testing rather than remote-control/Dock overlay rectangles; HID pointer routing; renderer background throttling disabled to preserve owner heartbeats; game background input through vanilla Minecraft handlers; game binding excludes competing desktop capture; aim selects a freshly observed visible block and rejects reuse after movement/action; nearby drops are bounded and distinguish visibility. Background play now suppresses automatic pause during its lease without altering the serialized player preference. Permissions, observation expiry, exact target checks and stop barriers remain enforced.

The restore smoke now waits for the restore button to become enabled, matching the real UI. Setting its hidden file input during export bypassed the disabled button and raced the export modal; the corrected full smoke passed without changing the product restore behavior.

Real model tests use a disposable profile with the user's configured model/API and a declared 64K test context window, plus local fixture pages and the existing temporary game world. The default user profile/API is unchanged. The game fixture was moved to a surface tree and given daylight/peaceful difficulty for bounded operation checks; no items were inserted and no world blocks were edited offline. Successful model goals were split into short instructions; autonomous exploration failed and is not accepted as complete. Raw API credentials/screenshots are not committed. Candidate, copied staging bundle and final installed bundle were each verified sequentially with strict/deep codesign checks before obsolete installs were removed. Formal Developer ID/notarization, clean-machine certification, Windows acceptance and long-term performance characterization remain separate follow-ups as agreed by the user.

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


Native authorization follow-up: macOS System Settings visibly reports both Screen Recording and Accessibility enabled for momo. A temporary TextEdit document was selected through the installed plugin UI and granted only once (no persistent allow-list entry). The first real model test failed: local `prepare_local()` sets `interrupt_hold` to pause phone activity, while the computer tool declaration guard incorrectly rejected the same owner run. Upstream 006c6a0 separates that phone hold from local application authority and preserves passive capture while suppressing proactive reactions. Regression coverage uses a real coordinator admission and approval lifecycle, with remote/old-run/audience/focus/revocation/epoch denials (45 related tests passed; focused rerun 30 passed). Packaged retest remains pending; OS toggles alone are not evidence of working capture or input. Test activity was stopped by returning to title and the temporary document discarded.


## Current native-control retest — 2026-11-06

Desktop 502c0ab / shared 44a93d4 was frozen, packaged, installed and passed the isolated native smoke. The actual model now receives computer tools. Real observation/input remains unverified: TCC first rejected stale ad-hoc code requirements. The user removed and re-added Accessibility and Screen Recording for the current app, then Quit & Reopen. Subsequent TCC checks no longer report the mismatch and window titles are available, but the real model observation still returned a generic failure and no input occurred. Returning to title ended the once-only activity; the temporary TextEdit document was discarded.

Shared 8b4ecf4 fixes independently reproduced scheduler races: background capture must not invalidate the model's observation version during an active turn, and an already-running capture must drain before model observation. Both regressions failed with the fix removed. Focused tests: 49 backend passed; 8 host passed, 1 installed-helper test skipped. Fixed allowlisted diagnostics now survive the protocol and tool boundaries; arbitrary exception text is withheld. This is not evidence that the installed failure is resolved: repackaging and actual native retest are required. Current grants expire after five minutes; session-duration authorization remains a follow-up design limitation.


## Observation budget correction — 2026-11-06

The 8b4ecf4 candidate passed frozen-model and isolated installed smoke, and was installed with its signatures verified. The user renewed Screen Recording and Accessibility for that candidate. A subsequent real observation within a fresh once-only grant produced the exact UI error `computer.budget`; no input was performed. This is a request-budget failure, not evidence of another OS permission failure.

Shared dc97894 corrects two budget inconsistencies: stored image cost metadata was stripped before a second estimate (falling back to 16,384 per image), and the pending screen observation was absent from compaction reservations. Structured costs now survive through the hard check; a size-only reservation participates in history image fitting and compaction, while pixels remain ephemeral and out of summaries. Capacity limits, generation/permission checks and image expiry remain enforced. The former failure was reproduced with a tiny stored image plus screenshot under a 16K window; 68 targeted regressions and three final pipeline/protection tests passed. Updated packaging and real installed observation/input remain pending.


## Consolidated desktop batch — 2026-11-06

Shared source `14c142d1a414bcbe8a938e6cf950ac287531885a` is promoted through the ownership manifest. Sound preview is inline, plans 014/015 are accepted, plugin cards share the QQ hierarchy and Chat directly exposes desktop mode and activity controls. The same Electron window/renderer/avatar/lease switches between normal Chat and a transparent always-on-top pet. Return is available in the pet UI, menu bar, Dock and recovery shortcut; click-through is cleared on recovery. Window restoration clamps to the current display after monitor changes. The menu follows the selected UI language.

Native capture reserves context before taking pixels and selects a bounded 1536/1024/512 maximum edge against the configured model capacity. Reservations use a square upper bound rather than stale window dimensions. Coordinates are mapped from the returned image to the exact authorized window; the observation version and 15-second expiry are preserved. Stop revokes pending approval and a failed native stop remains retryable. Approval and execution expiry are separate.

Isolated checks before packaging: 119 focused backend tests, 248 frontend tests and TypeScript, 26 shell tests and formatting/typecheck. Browser checks cover approval, denied/revoked state, stop retry, Minecraft attachment, Chat activity controls, same-renderer pet/return, three-language cards at two desktop sizes, and inline sound preview. Screenshot inspection caught and corrected a pet dialog width specificity issue. The final packaged candidate passed the full isolated native smoke, including pet transparency, dialog bounds, same canvas/lease and native bounds restoration. Its Electron CDP implementation lacks Browser.getWindowForTarget, so the test reads the actual renderer screen position and outer dimensions instead. The product binary did not change for that harness correction.

The installed app and real user profile have deliberately remained unchanged throughout this batch. Candidate packaging, frozen-model smoke and strict/deep signature verification passed for desktop `637380c7d4751adf94f0713564844cd4287aec4f` / shared `14c142d1a414bcbe8a938e6cf950ac287531885a`, CDHash `64b3cbdea37c78563d3f1a2fed37931f6f2db300`. Actual replacement of the installed app and real native observation/input remain pending; current TCC checkboxes do not establish authorization for the new ad-hoc-signed candidate. The isolated smoke also passed lifecycle, Boot/Chat/Settings/Home, synthetic replies, auth and backup round trip; it did not capture or control another application. Local evidence is archived under the 2026-11-06 batch-006-008-016-017-637380c backup directory. The task-owned remote build directory was removed after retrieval. No Developer ID identity is available. Minecraft real gameplay is pending the user's existing instance/loader confirmation (current adapter Fabric 1.21.1; standard local versions include 1.21.4/1.21.5). A browser tablet remains a proposal, not implemented scope. Plans 006/007/008/016/017 are not marked done.
