# Security Policy

NyaTerm handles credentials, private keys, OTP secrets, known hosts, cloud-sync
configuration, and AI or translation provider credentials. Please do not report
security issues in public issues or pull requests.

## Reporting

Use GitHub's private security advisory form for
[`nyakang/nyaterm`](https://github.com/nyakang/nyaterm/security/advisories/new)
when it is available. If private advisories are unavailable, contact the
maintainers through the private contact method listed in the repository owner's
profile and include `NyaTerm security report` in the subject. Do not attach real
passwords, private keys, OTP seeds, API tokens, or unredacted diagnostics;
provide a minimal reproduction and redact all secrets.

Please include:

- affected version or commit and operating system;
- the feature and observable impact;
- reproduction steps or a proof of concept that does not contain live secrets;
- any suggested mitigation.

We will acknowledge a report when received, validate the impact, coordinate a
fix and disclosure timeline with the reporter, and credit the reporter unless
they request anonymity. Please do not publicly disclose the issue before a fix
or coordinated disclosure has been agreed.

## Security expectations for contributors

Never commit or log passwords, decrypted credentials, private-key contents or
passphrases, OTP secrets or generated codes, cloud-sync/OAuth/API secrets, or
unredacted terminal and command context. Secret-bearing types must use redacted
`Debug` implementations when diagnostic output is required. Compatibility
readers must preserve unknown fields and must validate data before replacing
existing user data.

NyaTerm is provided under the Apache License, Version 2.0 and is not a
substitute for a security review of the host operating system or remote server.
