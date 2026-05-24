# TODO

- **Renew `WINGET_CREATE_GITHUB_TOKEN`** — classic PAT with `public_repo` scope, ≤ 90-day lifetime (Microsoft Open Source enterprise policy). Last set **2026-03-16**; renew by **2026-06-13**.

## Live mode stabilization

- [ ] Remove "(Beta)" label after 2 consecutive minor releases with zero Live-mode-related issues + multi-provider validation.
- [ ] Auto-reconnect on transient network drops.
- [ ] OS keyring migration for API keys (`tauri-plugin-stronghold` or `keyring-rs`).
- [ ] Secure-window auto-pause (UAC, lsass, credential dialogs).
- [ ] Additional streaming providers: Deepgram Nova-3, AssemblyAI Universal-Streaming.
- [ ] In-app cost meter / session cost estimation.
- [ ] Voice-command corrections ("scratch that", "delete last sentence").
