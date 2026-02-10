# Whisperi

A fast, modern desktop dictation app built with Tauri 2.x. Speak naturally and have your words transcribed, cleaned up, and pasted into any application.

## Why Cloud-First?

Whisperi primarily relies on cloud transcription services (OpenAI, Groq, Mistral) rather than local models. While local speech-to-text models like whisper.cpp exist, they require significant computational resources to achieve acceptable speed and accuracy. For most users, cloud APIs deliver near-instant, high-quality transcription that local models on consumer hardware simply cannot match.

## Features

- **Cloud Transcription** — OpenAI, Groq, and Mistral with model selection
- **AI Enhancement** — Post-process transcriptions with GPT, Claude, Gemini, or Groq models to clean up grammar, punctuation, and formatting
- **Auto-Paste** — Transcribed text is automatically copied to clipboard and pasted into the active window
- **Custom Dictionary** — Add names, jargon, and technical terms to improve accuracy
- **Agent Mode** — Say the agent name to switch from transcription cleanup to conversational AI
- **System Tray** — Runs quietly in the background with quick access via tray icon
- **Hotkey Support** — Tap-to-toggle or push-to-talk activation modes
- **Dark Mode** — Clean, minimal dark interface

## Tech Stack

- **Desktop Framework**: Tauri 2.x (Rust + WebView)
- **Frontend**: React 19, TypeScript, Tailwind CSS v4
- **Backend**: Rust with reqwest, rusqlite
- **Package Manager**: bun

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [bun](https://bun.sh/)
- Windows 10/11 (primary target)

### Development

```bash
# Install dependencies
bun install

# Start dev mode (Vite + Tauri)
bun run tauri dev

# TypeScript typecheck
bun run typecheck

# Rust tests
cd src-tauri && cargo test

# Production build
bun run tauri build
```

## Supported Providers

### Transcription
| Provider | Models |
|----------|--------|
| OpenAI | GPT-4o Mini Transcribe, GPT-4o Transcribe, Whisper |
| Groq | Whisper Large v3 Turbo |
| Mistral | Voxtral Mini |

### AI Enhancement
| Provider | Models |
|----------|--------|
| OpenAI | GPT-5.2, GPT-5 Mini, GPT-4.1 |
| Anthropic | Claude Sonnet 4.5, Claude Haiku 4.5, Claude Opus 4.5 |
| Google Gemini | Gemini 3 Pro, Gemini 3 Flash |
| Groq | Qwen3 32B, LLaMA 3.3 70B, Mixtral 8x7B |

## License

MIT
