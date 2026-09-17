<div align="center">

```
   ▄▄▄       █    ██ ▄▄▄█████▓ ▒██   ██▒   ██████ 
  ▒████▄     ██  ▓██▒▓  ██▒ ▓▒ ▒▒ █ █ ▒░ ▒██    ▒ 
  ▒██  ▀█▄  ▓██  ▒██░▒ ▓██░ ▒░ ░░  █   ░ ░ ▓██▄   
  ░██▄▄▄▄██ ▓▓█  ░██░░ ▓██▓ ░   ░ █ █ ▒    ▒   ██▒
   ▓█   ▓██▒▒▒█████▓   ▒██▒ ░  ▒██▒ ▒██▒ ▒██████▒▒
   ▒▒   ▓▒█░░▒▓▒ ▒ ▒   ▒ ░░    ▒▒ ░ ░▓ ░ ▒ ▒▓▒ ▒ ░
    ▒   ▒▒ ░░░▒░ ░ ░     ░     ░░   ░▒ ░ ░ ░▒  ░ ░
    ░   ▒    ░░░ ░ ░   ░        ░    ░   ░  ░  ░  
        ░  ░   ░                ░    ░         ░  
```

# AuthG

**The open-source, privacy-first desktop authenticator for Google Authenticator 2FA.**

[![License: MIT](https://img.shields.io/badge/License-MIT-000000?style=for-the-badge&logo=opensourceinitiative&logoColor=white)](LICENSE)
[![Website: authg.abhinavdhakal.com](https://img.shields.io/badge/Website-authg.abhinavdhakal.com-000000?style=for-the-badge&logo=googlechrome&logoColor=white)](https://authg.abhinavdhakal.com)
[![Support: BuyMeMomo](https://img.shields.io/badge/Support-Buy%20Me%20Momo-f43f5e?style=for-the-badge&logo=kofi&logoColor=white)](https://buymemomo.com/abhinavdhakal)
[![Platform: macOS & Windows](https://img.shields.io/badge/Platform-macOS%20|%20Windows%20|%20Linux-000000?style=for-the-badge&logo=apple&logoColor=white)](https://authg.abhinavdhakal.com)
[![Architecture: Tauri v2](https://img.shields.io/badge/Stack-Tauri%20v2%20+%20Rust%20+%20React-000000?style=for-the-badge&logo=tauri&logoColor=white)](https://tauri.app)
[![Security: AES-256-GCM](https://img.shields.io/badge/Security-AES--256--GCM%20Local%20Vault-000000?style=for-the-badge&logo=1password&logoColor=white)](https://authg.abhinavdhakal.com)
[![Offline: 100%](https://img.shields.io/badge/Offline-100%25%20Zero%20Telemetry-000000?style=for-the-badge&logo=adguard&logoColor=white)](https://authg.abhinavdhakal.com)

<br />

<p align="center">
  Stop digging for your phone every time you log in.<br />
  AuthG gives you instant, keyboard-first access to all your 2FA TOTP codes directly from your Menu Bar.
</p>

</div>

---

## Overview

Two-factor authentication is mandatory for security, but the mobile workflow breaks developer focus: unlocking phones, waiting on biometric prompts, and memorizing 6 digits under a 30-second ticking clock.

**AuthG** bridges this gap by bringing Google Authenticator to your desktop. It lives in your macOS Menu Bar or Windows System Tray, opens in one click, and computes mathematical TOTP tokens 100% offline.

```
                    ┌─────────────────────────────────────────┐
                    │       Google Authenticator (Phone)      │
                    │      Transfer accounts → Export QR      │
                    └────────────────────┬────────────────────┘
                                         │ Take screenshot
                                         ▼
                    ┌─────────────────────────────────────────┐
                    │               AuthG Desktop             │
                    │   Press ⌘V anywhere to import instantly │
                    └────────────────────┬────────────────────┘
                                         │
                 ┌───────────────────────┴───────────────────────┐
                 ▼                                               ▼
   ┌───────────────────────────┐                   ┌───────────────────────────┐
   │    Local Encrypted Vault  │                   │    Menu Bar Access        │
   │  PBKDF2 + AES-256-GCM     │                   │  One-click from top bar   │
   │  Stored on local SSD only │                   │  Click to copy 6 digits   │
   └───────────────────────────┘                   └───────────────────────────┘
```

---

## Features

- **Instant `⌘V` Paste Import**: Simply snap a screenshot of Google Authenticator's export QR code and hit `⌘V`. AuthG decodes protobuf payloads directly on your CPU in under a second.
- **100% Offline & Private**: Zero telemetry, zero analytics, and zero network calls. Your credentials never touch an external server or cloud database.
- **Hardware-Accelerated Encryption**: Vaults are encrypted using `AES-256-GCM` with keys derived via `PBKDF2-HMAC-SHA256` (600,000 iterations) from your master PIN.
- **Native Menu Bar & Tray Residency**: Built on Tauri v2 and Rust. Uses under 20MB of RAM and remains completely unobtrusive until clicked.
- **RFC 6238 Mathematical Compliance**: 100% identical to Google Authenticator. Compatible with HMAC-SHA1, SHA256, SHA512, 6-digit, and 8-digit TOTP tokens.
- **Privacy Mode**: Built-in screen blur hides 6-digit codes during Zoom meetings or in public coffee shops until you hover over them.
- **Guarded Account Details & Deletion**: Destructive actions are isolated behind dedicated confirmation flows to prevent accidental token loss.
- **Portable Backups**: Export or restore encrypted and JSON backups freely with zero vendor lock-in.

---

## Shortcuts & Controls

| Action | Scope | Description |
| :--- | :--- | :--- |
| `Menu Bar / Tray` | **Global** | Click icon to toggle AuthG popup |
| `⌘ + V` | **App** | Automatically parse & import QR screenshot from clipboard |
| `⌘ + K` or `/` | **App** | Instantly focus the account search filter |
| `Click Card` | **App** | Copy 6-digit TOTP code to clipboard |
| `Esc` | **App** | Navigate back to vault, clear search, or dismiss modals |
| `Right Click` or `(i)` | **App** | Open account details modal and safe deletion options |

---

## Security Architecture

```
User Master PIN
      │
      ▼
[ PBKDF2-HMAC-SHA256 ] ──( 600,000 rounds + machine salt )──► 256-bit Key
                                                                 │
Plaintext Vault JSON ────────────────────────────────────────────┼──► [ AES-256-GCM ] ──► authg_vault.enc
Random 96-bit IV     ────────────────────────────────────────────┘
```

1. **Key Derivation**: Master PINs are hashed using PBKDF2 with SHA-256 across 600,000 iterations to withstand offline GPU brute-force attacks.
2. **Authenticated Encryption**: Vault data is encrypted with `AES-256-GCM`. A 16-byte Poly1305 authentication tag verifies that the ciphertext has not been modified or corrupted.
3. **CSPRNG Entropy**: Nonces and salts are sampled directly from operating system entropy (`/dev/urandom` on macOS/Linux, `BCryptGenRandom` on Windows).
4. **Auto-Clearing Clipboard**: Copied 2FA codes are automatically wiped from the system clipboard after a customizable timeout (default: 30 seconds).

---

## Installation & Build

### Prerequisites

- [Node.js](https://nodejs.org) (v18+)
- [Rust & Cargo](https://rustup.rs) (1.78+)
- macOS Xcode Command Line Tools (`xcode-select --install`) or Windows C++ Build Tools

### Local Development

```bash
# Clone the repository
git clone https://github.com/abhidhakal/authg-app.git
cd authg-app

# Install frontend dependencies
npm install

# Run native desktop app in development mode
npm run tauri dev
```

### Running Tests

```bash
# Run Rust core tests (RFC 6238 vectors, Protobuf parser, AES vault roundtrip)
cd src-tauri
cargo test
```

### Production Build

```bash
# Compile optimized native binary (.dmg / .exe / .deb)
npm run tauri build
```

---

## Contributing

Contributions, issues, and feature requests are welcome. Feel free to open an issue or pull request on GitHub.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

---

## License

Distributed under the **MIT License**. See `LICENSE` for more information.

<div align="center">
  <sub>AuthG is an independent open-source project and is not affiliated with Google LLC.</sub>
</div>
