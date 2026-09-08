# OTP & Authentication

NyaTerm ties OTP management into the SSH authentication flow. You can manage OTP entries as reusable credentials, then bind them directly to SSH connections to reduce repeated input.

## Supported OTP types

NyaTerm currently supports:

- **TOTP**
- **HOTP**

When creating or editing an OTP entry, you can configure:

- Issuer
- Username
- Secret
- Algorithm (`SHA-1`, `SHA-256`, `SHA-512`)
- Digits
- Period for TOTP
- Counter for HOTP

## Where to manage OTP

Open the **OTP** tab inside the **Security/Auth** panel.

There you can:

- Create OTP entries
- Edit existing entries
- Delete entries
- View current verification codes
- Import from a QR code image

## Import from QR code

If you already have an MFA / 2FA QR code, you can import the image directly.

Typical flow:

1. Click the QR import action in the OTP panel
2. Choose a local image file
3. NyaTerm parses and fills fields such as issuer, username, and secret
4. Confirm and save the OTP entry

This is usually more convenient than manually retyping the secret.

## Bind OTP to an SSH connection

In the advanced section of the SSH connection form, you can select a saved OTP entry for that connection.

After binding, you can:

- Quickly inspect the current code during login
- Enable **auto-fill OTP** in compatible interactive authentication flows

This covers environments that require a password or private key plus a second factor.

## OTP interaction during authentication

When the SSH server enters a keyboard-interactive or OTP flow, NyaTerm shows an OTP dialog.

The dialog includes:

- The current connection name
- The prompts requested by the server
- A code panel if the connection is bound to an OTP entry

You can then:

- Enter the verification code manually
- Send the current OTP code into the prompt
- Submit or cancel the authentication attempt

## Auto-fill OTP

If an SSH connection is already bound to an OTP entry, you can enable **auto-fill OTP**.

Good fits include:

- Stable infrastructure environments
- Bastion hosts that always require OTP
- High-frequency operational logins

It is best enabled only on connections whose authentication prompts you understand well, so you avoid filling the wrong value into an unexpected interactive prompt.

## OTP with passwords and keys

OTP does not replace passwords or private keys. It is used alongside them:

- **Password + OTP**
- **Private key + OTP**

A clean workflow is:

1. Organize passwords, keys, and OTP entries in **Security/Auth**
2. Bind them to specific SSH connections afterward

Connection records stay cleaner, and credentials can be replaced in one place later.
