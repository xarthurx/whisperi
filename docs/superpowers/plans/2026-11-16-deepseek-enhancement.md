# DeepSeek Enhancement Implementation Plan

**Goal:** Add a direct DeepSeek API option to text enhancement.

**Design:** Reuse the existing provider selector and OpenAI-compatible request path. Add a separate `deepseekApiKey`, with `deepseek-flash` selected first and `deepseek-v4-pro` as an alternative. Send `thinking: { "type": "disabled" }` only to `https://api.deepseek.com`. Keep the existing prompts, intensity controls, language handling, and failure fallback. DeepSeek is an enhancement provider only.

**API references:** [First API call](https://api-docs.deepseek.com/), [Chat Completions](https://api-docs.deepseek.com/api/create-chat-completion/), and [Models](https://api-docs.deepseek.com/quick_start/pricing/), checked 2026-11-16.

## Tasks

- [x] Wire the settings key, provider display name, icon, enhancement tab, and model registry. Confirm it is absent from transcription and Live provider pickers.
- [x] Add backend routing and endpoint-scoped non-thinking configuration. Verify request options and unchanged behavior for other providers.
- [x] Verify provider/key integration, the frontend build, and Rust library tests. Review failure handling for empty and truncated replies. WebSocket integration tests are blocked by local socket permissions (`os error 10013`).
- [x] Update changelog, architecture, progress, and follow-up validation tasks with final behavior and verification results.

## Review focus

- API keys load, save, and synchronize through the existing settings path.
- Selecting DeepSeek changes the model to a valid DeepSeek model.
- Provider-specific request fields are absent from other providers' requests.
- Standard and Live final enhancement both use the generic enhancement path.
- A failed enhancement retains the transcript through the existing fallback.

## Execution notes

- Preserve the pre-existing changes in `docs/PROGRESS.md` and `scripts/submit-winget.ps1`.
- Initial scope was a source change. The user subsequently authorized publishing a new version through the existing workflow; release preparation targets `v0.9.2`. Do not change the active user's stored settings.
