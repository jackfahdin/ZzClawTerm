# Security

NyaTerm's security features mainly focus on three areas:

1. Safely storing local credentials and authentication materials
2. Managing host verification and second-factor flows during SSH login
3. Providing master-password and screen-lock protection in the desktop workspace

## How sensitive local data is stored

NyaTerm stores connection-related configuration locally, but sensitive values are encrypted before being written to disk. Typical sensitive data includes:

- Saved passwords
- SSH private keys and key passphrases
- OTP secrets
- The persisted representation of the master password
- Proxy or other authentication materials that need protection

So in day-to-day use, you work with reusable password, key, and OTP entries rather than scattering plaintext secrets through config files.

## The Security/Auth panel

The **Security/Auth** panel in the left activity bar centralizes authentication-related records into three groups:

- **Keys**
- **Passwords**
- **OTP**

This means you do not need to re-enter every secret in every connection. You save reusable entries first, then reference them from the connection form.

### SSH key management

Good for storing:

- Common login keys
- Keys protected by passphrases
- Multiple identities separated by environment

When you switch an SSH connection to **Private Key** authentication, you can pick from these saved keys directly.

### Password management

Good for storing:

- SSH passwords
- Proxy passwords
- Other credentials you need to reuse

In the SSH connection form, password authentication can reference these saved password entries directly.

### Credential auto-fill

Credential management is separate from SSH password authentication and can respond to password prompts in an interactive terminal.

1. Create an entry with a name, prompt-matching regular expression, value to send, and enabled/disabled state.
2. When terminal output matches the prompt expression, NyaTerm shows an autofill prompt.
3. You can approve the fill or ignore it.

Values remain encrypted locally, viewing saved passwords requires unlocking sensitive data, and autofill only triggers after a prompt match rather than sending values unconditionally.

### OTP management

OTP management supports:

- **TOTP**
- **HOTP**
- Import from QR code images
- Viewing and generating current codes
- Binding OTP entries to SSH connections

For details, see [OTP & Authentication](./otp-and-auth).

## Master password

The master password is NyaTerm's most important local desktop protection feature.

You can configure it in **Settings → Security**. After it is set:

- The app uses it for unlock verification
- Sensitive local configuration protection is built around it
- The lock screen requires it before unlocking
- **Cloud Sync** actions become eligible to be enabled and used

If you have not set a master password, the lock screen is only a visual lock layer, not full password-based protection.

## Cloud Sync security model

If you plan to use **Settings → Cloud Sync**, the master password is not just recommended. It is a prerequisite.

In the current implementation:

- Cloud Sync actions cannot be enabled without a master password
- Cloud-provider credentials are treated as protected local secrets
- NyaTerm uploads **encrypted portable snapshots**, not plain-text config files
- Pulling from cloud overwrites the portable local data included in the snapshot, but it does not blindly roam every piece of device-local UI state to another machine

That makes this feature best understood as:

- **Cross-device sync for portable configuration**
- **Local `.nya` export for backup and migration**

not as a collaborative merge tool.

For the complete setup and workflow details, see [Cloud Sync](./sync-and-backup).

## Screen lock and app lock

### Manual lock

You can trigger lock from the UI or by keyboard shortcut at any time.

### Auto lock

In **Settings → Security**, once screen lock is enabled, you can also configure the idle timeout:

- `0` means no idle auto-lock
- Values greater than `0` trigger automatic lock after that many idle minutes

### Unlock behavior

- **With a master password** — entering the correct master password is required
- **Without a master password** — unlocking can happen directly

The app lock state is shared across the main workspace, modal child windows, and idle detection. When locked, terminal content and sensitive panels are covered; treat it as protection for the local NyaTerm workspace and local sensitive-operation entry points, not as remote-system access control.

If you plan to use NyaTerm on a shared machine or during demos, enabling **master password**, **screen lock**, and **idle auto-lock** together is the safer setup.

## SSH host key policies

When SSH first connects to an unknown host, NyaTerm supports three policies:

| Policy | Behavior |
|------|------|
| Prompt | Ask whether to trust the host key on first connect (default) |
| Accept | Automatically accept new host keys |
| Strict | Reject all unknown host keys |

Known host records are stored in the local redb `known_hosts` document. Legacy `known_hosts` content from the old redb text-document path can be imported through the compatibility reader; NyaTerm does not promise to scan arbitrary filesystem locations for an old `known_hosts` file.

If host identity validation matters in your environment, prefer **Prompt** or **Strict** over unconditional acceptance.

## Local persistence model

In installed mode, the main local data file is normally `~/.nyaterm/nyaterm.redb`. Portable mode stores configuration under the adjacent `data/config/` directory. Legacy Dragonfly data is handled through compatibility fallbacks for old encryption prefixes and storage documents; NyaTerm does not promise to copy the entire `~/.dragonfly/` directory or create an automatic rollback copy.

The **Export Configuration** action creates an encrypted `.nya` backup for migration or offline storage. The **Export Diagnostics** action is intended for troubleshooting and packages recent logs and runtime summaries; review diagnostic contents before sharing them.

## Practical security advice

- Prefer **private key + OTP** in production environments instead of relying on one password
- Enable both **master password** and **screen lock** on shared computers or demo machines
- Save **jump host** and **proxy** definitions explicitly for environments that depend on them
- Verify the source of a new host key before trusting it
