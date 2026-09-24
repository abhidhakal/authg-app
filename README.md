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
   │  AES-256-GCM, key in the  │                   │  One-click from top bar   │
   │  system keystore          │                   │  Click to copy 6 digits   │
   └───────────────────────────┘                   └───────────────────────────┘
```

---

## Features

- **`⌘V` Paste Import**: Screenshot Google Authenticator's export QR code and press `⌘V`, or scan it with your webcam. Decoded locally.
- **Type, Enter, Done**: Search opens on the top match, so typing part of a name and pressing Enter copies its code.
- **Never Paste an Expired Code**: In the last seconds of each code, the next one is shown too. Accounts you use most stay at the top.
- **Encrypted Local Vault**: AES-256-GCM. On macOS and Windows the key is kept in the system keystore (Keychain / Credential Manager), not in the vault file.
- **Native Menu Bar Popup**: Opens under its icon, closes when you click away, follows you across Spaces. No Dock clutter.
- **Standard TOTP (RFC 6238)**: SHA1, SHA256, SHA512, 6 and 8 digits. HOTP (counter-based) accounts aren't supported yet.
- **Privacy Mode**: Blurs codes until you hover over them, for screen sharing.
- **JSON Backups**: Export or restore anytime, no lock-in. The export file is **not** encrypted, so store it carefully.
- **Offline**: No telemetry or analytics. The only network request is the update check, when you click it.

---

## Shortcuts & Controls

| Action | Scope | Description |
| :--- | :--- | :--- |
| `Menu Bar / Tray` | **Global** | Click icon to toggle AuthG popup |
| `⌘ + V` | **App** | Automatically parse & import QR screenshot from clipboard |
| `⌘ + K` | **App** | Focus the account search |
| `Type` + `Enter` | **App** | Copy the top search match |
| `↑` / `↓` + `Enter` | **App** | Move between accounts and copy |
| `Click Card` | **App** | Copy 6-digit TOTP code to clipboard |
| `Esc` | **App** | Navigate back to vault, clear search, or dismiss modals |
| `Right Click` or `(i)` | **App** | Open account details modal and safe deletion options |
| `⌘ + Q` | **App** | Quit AuthG |

---

## Security Architecture

```
Random 256-bit key ──► macOS Keychain / Windows Credential Manager
                                   │
Vault JSON ──────► [ AES-256-GCM, fresh random nonce ] ──► vault.enc (0600)
```

1. **Key storage**: On first launch AuthG creates a random 256-bit key and stores it in the system keystore. The vault file never contains it, so a copied `vault.enc` is useless on its own.
2. **Encryption**: The vault is encrypted with `AES-256-GCM` with a fresh random nonce on every save. Tampering makes decryption fail.
3. **Linux**: No keystore yet. The vault is encrypted, but the key can be recomputed from the file, so rely on disk encryption.
4. **Clipboard**: Copied codes are marked concealed (clipboard managers skip them) and cleared after 30 seconds by default, if the clipboard still holds that code.

---

## Download & Installation

### Pre-built Binaries

You can download pre-built installers for macOS (Apple Silicon & Intel), Windows, and Linux directly from:
- **Official Website:** [authg.abhinavdhakal.com/download](https://authg.abhinavdhakal.com/download)
- **GitHub Releases:** [github.com/abhidhakal/authg-app/releases](https://github.com/abhidhakal/authg-app/releases)

### Homebrew (macOS)

Technical users on macOS can install via Homebrew Cask:

```bash
brew install --cask abhidhakal/tap/authg
```

### First launch on macOS

AuthG is signed but not notarized by Apple (that needs a paid developer account), so macOS blocks the first launch. Open **System Settings → Privacy & Security** and click **Open Anyway**, or run `xattr -cr /Applications/AuthG.app`. This is only needed once; updates install without it.

---

## Development & Build

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
