# PV Studio

Desktop photovoltaic design application — **3D performance prototype**.
Tauri 2 + Rust core + React/TypeScript + React Three Fiber.

## Prerequisites

| Tool | Version |
|---|---|
| Rust | 1.99.1 (pinned in `rust-toolchain.toml`, installed via rustup) |
| Node.js | ≥ 26.13 |
| pnpm | 12.16.1 |
| Windows | MSVC build tools + Windows SDK (WebView2 ships with Windows 11) |
| macOS | Xcode command-line tools |

## Getting started

```sh
pnpm install
pnpm dev          # runs `tauri dev` (Vite on :1420 + the Rust shell)
```

## Quality gates

```sh
pnpm typecheck                               # tsc --noEmit (strict) in every package
pnpm test                                    # Vitest
pnpm lint                                    # ESLint + Prettier check
pnpm i18n:check                              # fr / en / de key parity
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
