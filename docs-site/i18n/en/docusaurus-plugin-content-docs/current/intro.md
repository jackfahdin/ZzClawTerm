---
slug: /
---

# Introduction

**NyaTerm** is a native desktop client built with GPUI around remote terminal workflows. Its Rust workspace combines the desktop interface, terminal rendering, SSH, SFTP, session lifecycle, tunnels, authentication, AI features, Cloud Sync, and configuration persistence, so you can work with remote servers, local shells, serial devices, and network helpers inside one workspace.

## Where NyaTerm fits best

- Managing multiple SSH hosts at the same time
- Switching between local terminals, Telnet sessions, and serial devices during troubleshooting
- Working with remote files while watching terminal output
- Standardizing common operations with reusable commands, jump-host chains, and saved connection metadata
- Using OTP, recording, resource monitoring, auto-upload, AI assistance, cross-device config sync, and local encrypted backup in the same desktop app
- Migrating existing configuration from Xshell, MobaXterm, WindTerm, SecureCRT, FinalShell, Termius, or NyaTerm backup files

## Core capabilities

### Multiple session types

NyaTerm currently supports six session types:

- **SSH** — remote login, file transfer, resource monitoring, tunnels, OTP, and related workflows
- **Local Terminal** — open a local shell inside the same workspace
- **Telnet** — support for legacy systems and lab environments
- **Serial** — network gear, embedded boards, and debug ports
- **RDP** — Windows Remote Desktop and graphical administration
- **VNC** — traditional RFB/VNC services and VM consoles; real-server interoperability is still being validated

### Composable workspace

- Multi-tab workflow for different tasks and environments
- **Horizontal and vertical splits** inside a tab, with drag docking for moving tabs into target split areas
- Terminal layout restoration and Workspace Padding for clearer terminal spacing
- Command Palette and session quick switcher for finding actions, sessions, and saved connections
- Left and right activity bars for file explorer, network, Security/Auth, Cloud Sync, settings, AI Assistant, saved connections, active sessions, command history, and resource monitor panels
- Bottom helper areas for quick commands, serial send, recording, and lock controls
- Separate child windows for settings, new-session, quick-command editing, remote-file editing, and auto-upload prompts
- Tray minimize and hide-main-window behavior for background workflows

### Terminal-focused enhancements

- Command history and fuzzy suggestions, with automatic suppression in interactive programs
- Configurable history-command length filters to reduce noise from very long commands
- Terminal search, search history, result counts, copy/paste, and context actions
- Terminal zoom, font weights, workspace spacing, macOS IME compatibility, and image path pasting
- **Online search** and **translation** from selected terminal text
- Optional **line-number / timestamp gutter**
- Optional **action links** for IPv4 addresses, `host:port`, and archive names
- Optional **keyword highlighting** with built-in presets and custom rules
- Large-output protection, session recording, and SSH keep-alive
- In-terminal **Zmodem** (`rz`/`sz`) and **trzsz** (`trz`/`tsz`) file transfer

### Remote file and transfer workflows

- Built-in SFTP file explorer for SSH sessions
- Upload, download, rename, move, delete, properties, and OpenSSH-compatible symlink actions
- Transfer queue with speed display, pause, resume, cancel, retry, duplicate-target handling, timestamp preservation, and configurable concurrency
- Open a remote file in a local editor, then send changes back through the watcher-driven auto-upload flow
- External drag-and-drop uploads from the system file manager on Windows

### AI Assistant and automation

- Built-in **AI Assistant** panel in the right activity bar
- **Ask** mode for one-off command generation, output explanation, and error analysis
- **Agent** mode for multi-step command execution against the active terminal session
- Agent command execution and final answers are separated so command output remains auditable
- Built-in providers, custom **OpenAI Compatible** providers, manual model entries, and credential groups
- Structured command cards with risk controls, approvals, and save-to-quick-command support

### Security and networking

- Passwords, private keys, host-key policies, and encrypted local storage
- Credential management with regex-based terminal password auto-fill
- OTP management with TOTP/HOTP, QR import, and SSH auto-fill support
- SOCKS5, HTTP, ProxyCommand, validated jump-host chains, local / remote / dynamic tunnels, and SSH X11 forwarding
- Screen lock, master password, and idle app lock support
- Import **Xshell / MobaXterm / WindTerm / SecureCRT / FinalShell / Termius** sessions, plus NyaTerm/Electerm JSON
- Encrypted `.nya` import / export for NyaTerm configuration backups
- Open log folders and export diagnostic bundles for troubleshooting and support

### Cloud Sync

- Seven remote storage backends: **WebDAV**, **S3-compatible** storage, **Gitee Snippet**, **GitHub Gist**, **Google Drive**, **OneDrive**, and **AliyunDrive**
- Configure a master password in **Settings → Security** before using **Settings → Cloud Sync**
- Support startup checks, debounced auto-push after supported local changes, and manual push / pull
- Resolve snapshot-level conflicts from the settings page or the in-workspace history panel when both local and remote state changed

## Suggested reading order

If you are new to NyaTerm, this order works well:

1. [Quick Start](./getting-started/quick-start)
2. [Session Types](./guide/session-types)
3. [SSH Connection Management](./guide/ssh-connection)
4. [Layout and Workspace](./guide/layout-and-workspace)
5. [Terminal Features](./guide/terminal)
6. [SFTP File Transfer](./guide/file-transfer)
7. [Tunnels and Proxy](./guide/tunnels-and-proxy)
8. [OTP and Authentication](./guide/otp-and-auth)
9. [AI Assistant](./guide/ai-assistant)
10. [Security](./guide/security)
11. [Cloud Sync](./guide/sync-and-backup)
