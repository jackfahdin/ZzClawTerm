# AI Assistant

NyaTerm includes an AI Assistant panel that can start from terminal context, selected text, file operations, or a manual prompt. It supports two working modes: **Ask** and **Agent**.

## Working modes

### Ask mode

Ask is the default mode and fits one-off help:

- Generate commands
- Explain terminal output
- Analyze errors
- Derive a fix command from selected text

Commands returned by AI are shown as structured cards with risk labels.

### Agent mode

Agent mode lets AI perform multi-step work. It uses a ReAct-style loop: observe terminal output → decide the next command → execute → observe again until the task is complete or a step limit is reached.

Agent mode characteristics:

- Requires an active terminal session
- Shows each command's state: running, completed, failed, timed out, needs approval, or blocked by policy
- Configurable maximum steps and per-step timeout in **Settings → AI → Agent Settings**
- Uses `Terminal Output Lines` to control how much AI-executed command output is shown inline near the terminal
- High-risk commands still require manual approval before execution

## Agent tools and final answers

The Agent workflow separates "run terminal commands" from "summarize the result":

- Agent can propose commands to run in the active terminal session
- Commands still go through risk levels, policy checks, and manual approval
- Execution state is recorded, and output summaries can be shown near the terminal according to `Terminal Output Lines`
- After terminal steps finish, Agent uses a final-answer tool to provide the user-facing summary instead of mixing it into command output

With the two split apart, the terminal keeps the real execution record while the final answer explains the outcome and next steps, so multi-step troubleshooting and deployment checks are easier to review afterwards.

## Conversation management

AI Assistant supports multiple conversations:

- **New conversation** creates an independent context
- **History** groups conversations by time, such as today, yesterday, last 7 days, and older
- **Search history** fuzzy-searches conversation content
- **Delete conversation** removes history you no longer need

## Session mentions

Type `@` in the input box to mention other terminal sessions and bring their context into the current AI conversation, for cross-session analysis or comparison.

## Command cards and risk control

AI commands are displayed as structured cards with:

- Command text
- Risk level: low / medium / high / critical
- Execute or approval actions
- Save-as-quick-command option

### How the risk level is decided

The risk level is not the model's call alone. NyaTerm takes the **higher** of the model's self-reported level and the **local rule verdict** as the effective level, so a model that underestimates risk still gets stopped by local rules. The command card lists both sources.

Local rules use four tiers, and they classify by command *shape* rather than command name:

| Level | Examples |
|-------|----------|
| Critical | `rm -rf /`, `mkfs`, `wipefs`, `dd` to a block device, fork bombs, `shutdown` / `reboot` / `poweroff`, stopping sshd |
| High | anything starting with `sudo`, `rm -r` / `rm -f`, `chmod -R` / `chown -R`, package installs and removals, `docker rm` / `system prune`, `kubectl delete` / `drain` / `apply`, `git reset --hard` |
| Medium | bare `chmod` / `chown`, redirects `>` / `>>`, `cp` / `mv` / `mkdir` / `touch`, `git pull` / `merge`, `npm run`; anything matching no rule also lands here |
| Low | read-only diagnostics such as `ls`, `cat`, `ps`, `df`, `docker ps`, `kubectl get`, `git status` |

Note that tiers look at command shape: bare `chmod` / `chown` is medium and only the recursive `-R` flag raises it to high; `rm -rf /` is critical while `rm -rf ./build` is high.

Commands you type manually go through the same rules — see [Terminal Features → Command risk assessment](./terminal#command-risk-assessment).

### Command execution policy

The effective level decides whether a command may run, together with **Command execution policy** in **Settings → AI → Agent Settings**:

| Policy | Behavior |
|--------|----------|
| Confirm before execution | Every command needs your confirmation |
| Fully automatic | No confirmation; commands run directly |
| Smart approval | Runs automatically when the effective level is at or below **Smart mode auto-execute ceiling**, otherwise waits for approval |

**Smart approval** has one hard rule: **critical risk always requires manual confirmation**, even with the auto-execute ceiling raised to its maximum.

**Settings → AI** also configures:

- **Smart mode auto-execute ceiling**
- Whether command risk checking is enabled
- Whether generated commands may be saved as quick commands

### Safer alternatives

When a command is judged high risk, AI may also provide a safer alternative command.

## Recent output and inline terminal output

AI Assistant can work around "what just happened" instead of only selected text.

- Use **Explain recent output** from the terminal context menu to send recent output from the current session to AI
- In Agent mode, AI command execution events are captured and summarized near the terminal workflow
- `Terminal Output Lines` controls the maximum number of inline feedback lines; set it to `0` to disable them

## Reasoning content

If the selected model exposes a reasoning channel, such as DeepSeek R1 or QwQ-style models, AI Assistant can show reasoning content in the response. Reasoning content is collapsed by default and can be expanded when needed.

## Provider and model configuration

Manage providers and models in **Settings → AI**.

### Provider configuration

- Built-in providers such as OpenAI, Anthropic, Google, and DeepSeek
- Custom **OpenAI Compatible** providers
- Each provider needs an API key and optional Base URL

### Model management

- Fetch available models from providers
- Manually add models that a provider does not return automatically
- Group models by provider and credential group
- Enable / disable individual models
- Choose a default model

### Other settings

- **Context lines**: maximum terminal output lines sent to AI
- **Request timeout**: timeout for one AI request
- **Record history**: whether to save AI conversation history
- **Sensitive redaction**: redact sensitive content before sending

## Invoke from terminal and file explorer

AI Assistant can be invoked from context menus, not only from the panel input.

### Terminal context menu

- **Explain recent output** sends recent output from the current session
- **Explain selected text** explains the selected log, error, or fragment
- **Analyze error** asks AI to generate repair suggestions from an error context
- **Fix selected text** derives the next command from selected error text
- **Generate command** asks AI to produce a command for your goal

### File explorer context menu

For files in the SFTP file explorer, you can send file content to AI for analysis from the context menu. File size limits apply and can be adjusted in settings.

### Error auto-detection

When terminal output matches error patterns, AI Assistant can suggest analyzing the error automatically.
