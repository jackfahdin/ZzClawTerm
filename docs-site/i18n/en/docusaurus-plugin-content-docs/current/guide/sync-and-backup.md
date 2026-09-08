# Cloud Sync

NyaTerm's **Cloud Sync** capability is not remote file transfer. It syncs portable application configuration through one encrypted current snapshot in remote storage.

The remote side is intentionally current-state oriented:

- `sync/current.redb.enc` stores the encrypted current core snapshot
- `sync/latest.redb` stores revision, device, timestamp, and hash metadata
- legacy `sync/snapshots/` objects are only used for compatibility reads and cleanup

Local `.nya` export / import remains the backup and migration path. Cloud Sync no longer creates remote multi-version backups, and it does not automatically delete any old `backups/` objects that may already exist in user storage.

NyaTerm currently supports these remote storage provider types:

- **WebDAV**
- **S3-compatible** storage
- **Gitee Snippet**
- **GitHub Gist**
- **Google Drive**
- **OneDrive**
- **AliyunDrive**

## Prerequisite: set a master password first

Before using **Settings → Cloud Sync**, you must first set a **Master Password** in **Settings → Security**.

Without a master password:

- Cloud Sync cannot be enabled
- Manual actions such as test, push, and pull cannot be run

That requirement exists because NyaTerm uploads encrypted portable snapshots, not plain-text config files.

## Where to access it

This feature has two main entry points.

### 1. Settings → Cloud Sync

Use the settings page to:

- choose a provider
- configure the remote namespace and provider root
- define automatic sync behavior
- run test connection, push now, and pull now
- resolve conflicts when they happen

### 2. The Cloud Sync panel in the workspace

The workspace **Cloud Sync** panel shows:

- current status
- recent sync activity
- last check and sync times
- conflict state and quick resolution actions

Use the settings page to configure the feature, and the workspace panel to watch its live state.

## Provider configuration

### Common fields

No matter which provider you use, start by checking these fields:

- **Enable Cloud Sync**
- **Provider Configuration**
- **Device Name**
- **Remote Namespace**

These have specific meanings:

- **Device Name** is written into snapshot metadata so you can tell which device uploaded it
- **Remote Namespace** is the top-level path prefix NyaTerm uses for the cloud sync snapshot inside the selected provider

### WebDAV

When the provider is **WebDAV**, you will typically configure:

- **WebDAV Endpoint**
- **Provider Root** (optional)
- **Username**
- **Password**

WebDAV authentication supports both **Basic** and **Digest** auth. NyaTerm switches to Digest when the server requires it, improving compatibility with NAS / gateways that mandate Digest.

### S3-compatible storage

When the provider is **S3-compatible**, you will typically configure:

- **S3 Endpoint**
- **Bucket**
- **Region**
- **Provider Root** (optional)
- **Access Key ID**
- **Secret Access Key**
- **Session Token** (optional)
- **Virtual Host Style** (enable only if your provider requires it)

### Gitee Snippet

When the provider is **Gitee Snippet**, NyaTerm stores encrypted cloud sync objects in a Gitee code snippet.

You will typically configure:

- **Gitee Personal Access Token**
- **Snippet ID**

### GitHub Gist

When the provider is **GitHub Gist**, NyaTerm authorizes GitHub with device flow and stores encrypted cloud sync objects in a private Gist.

You will typically:

- click **Connect GitHub**
- enter the device code on the GitHub authorization page
- grant the `gist` scope

During the GitHub device-flow authorization, NyaTerm creates and fills in a private Gist when no Gist ID is configured. When a Gist ID already exists, it validates and reuses that Gist.

### Google Drive / OneDrive / AliyunDrive

When the provider is **Google Drive**, **OneDrive**, or **AliyunDrive**, NyaTerm uses native desktop HTTP adapters to access the selected drive service.

The current implementation expects OAuth credentials to be entered manually:

- **Refresh Token**
- **Client ID**
- **Client Secret**
- **Access Token** (optional; refresh token is recommended for long-term sync)
- **Provider Root** (optional)

AliyunDrive also requires **Drive Type**, usually `resource`.

### Validation rules

The current implementation performs basic provider validation before actions are allowed:

- WebDAV requires an endpoint
- S3 requires both an endpoint and a bucket
- `Access Key ID` and `Secret Access Key` must be provided together
- GitHub Gist requires GitHub authorization; the Gist ID can be created and filled in during authorization or supplied for an existing Gist
- Google Drive / OneDrive / AliyunDrive require Refresh Token, Client ID, and Client Secret

A good habit is to run **Test Connection** before enabling any automatic strategy.

## Automatic sync strategy

### Check on startup

When enabled, NyaTerm checks the remote provider during startup to see whether a newer sync snapshot already exists.

Typical outcomes include:

- local and remote are already aligned
- the remote side has a newer snapshot available to pull
- local changes are pending upload
- both local and remote changed, so a conflict is detected

This is not real-time remote watching. It is a startup check of the current state.

### Auto-pull remote changes

When **Auto Pull Remote Changes** is enabled, if startup check or sync check finds that only the remote side changed, NyaTerm automatically pulls and applies the current remote snapshot.

This only handles cases with no unsynchronized local changes. If both local and remote changed, NyaTerm still enters conflict state instead of choosing one side automatically.

### Auto-push after local changes

When enabled, NyaTerm automatically pushes a snapshot after supported local configuration changes are saved, using a debounce window.

You can control:

- **Sync Debounce Seconds**

That makes the behavior closer to "push shortly after save" than to real-time bidirectional sync.

## Manual actions

### Test Connection

Validate the provider configuration and confirm the remote layout is reachable.

### Push Now

Upload the current device's portable configuration snapshot to the cloud.

Useful when:

- you just finished a meaningful batch of settings changes
- you want explicit control over when cloud state is updated

### Pull Now

Fetch the current latest sync snapshot from the cloud and apply it locally.

Useful when:

- another device has already pushed an update
- the current device needs to align with the latest remote state on demand

## How conflicts happen

A conflict occurs when both local state and remote state changed since the last sync baseline.

A typical example looks like this:

- Device A changed settings but has not pulled Device B's update
- Device B already pushed a newer sync snapshot to the cloud
- Device A then tries to push or performs a startup check and discovers both sides changed

When this happens, NyaTerm can show conflict details such as:

- local snapshot hash
- remote revision
- remote device information
- current remote snapshot time and hash
- a human-readable conflict message

## How conflict resolution works

The current implementation offers two resolution actions.

### Download Remote Version

Pull the remote snapshot and apply it to the current device.

Use this when:

- the cloud copy is the version you want to keep
- the current device's local changes should be discarded

### Upload Local Version

Force the current device's local snapshot to become the new remote state.

Use this when:

- the current device has the correct latest version
- you want other devices to follow this state later

The important boundary is:

- this is not a field-level merge
- this is not collaborative conflict resolution
- it is effectively a choice between the local snapshot and the remote snapshot

NyaTerm keeps metadata for the current remote snapshot to help you decide whether to download the remote version or upload the local version. Legacy `sync/snapshots/` objects are only used for compatibility reads and cleanup, not as the normal multi-version backup entry point.

## What gets synced?

Cloud Sync is built on portable snapshots. These cover NyaTerm's portable configuration data, such as:

- saved connections and groups
- key, password, and OTP configuration
- proxies and tunnels
- quick commands
- most application settings
- known_hosts and the master-key token

Command history is not included in the cloud sync core snapshot. Use local `.nya` export / import when you need an offline backup or migration package.

The current implementation deliberately preserves some device-local UI state, such as:

- currently open tabs
- live panel open/close and sizing state

So the feature is best understood as:

- syncing portable configuration
- letting multiple devices share the current core configuration state

not as a full desktop-session image.

## Recommended first-time setup order

If you are enabling this for the first time, this order works well:

1. Set a master password in **Settings → Security**
2. Open **Settings → Cloud Sync**
3. Choose the provider, then fill in endpoint / bucket / root / credentials
4. Set the device name and remote namespace
5. Run **Test Connection** first
6. Then decide whether to enable:
   - check on startup
   - auto-pull remote changes
   - auto-push
7. Finally run **Push Now** once to verify the full path works

## Troubleshooting hints

If buttons are disabled or actions are blocked, check these first:

- whether a master password has been configured
- whether current settings have been saved or applied
- whether provider-required fields are complete
- whether the remote namespace, provider root, and endpoint values are correct

If your main question is "what happened recently?", start with the workspace **Cloud Sync** panel. If your main goal is changing config, go back to the settings page.

:::tip Practical advice
- When connecting a new provider for the first time, run **Test Connection** before enabling automatic strategies.
- Before making large changes to connections, OTP, or quick commands, use local `.nya` export if you need a rollback point.
- If you switch between devices often, keep a clear device-name convention and a predictable remote-namespace strategy.
:::
