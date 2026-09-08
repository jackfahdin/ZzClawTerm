# Quick Commands

Quick Commands let you save common commands as reusable actions, then send them to the current terminal from inside the workspace.

## Good use cases

- Frequently repeated operational commands
- Deployment or troubleshooting scripts with parameters
- Organizing commands by product, environment, or team
- Placing risky commands into the prompt first so you can review them before execution

## Create a quick command

1. Open the **Quick Commands** area in the bottom helper section or side panel
2. Click **Add**
3. Fill in the command details in the dedicated child window

Available fields include:

| Field | Description |
|------|------|
| Label | Display name for the command |
| Category | Command grouping |
| Description | Optional note |
| Color Tag | Custom display color |
| Icon | Custom icon |
| Pin to Top | Whether it stays near the top of the list |
| Execution Mode | Execute immediately or append to the input line |
| Command Script | The command text to send |

After saving, the command appears in the list and can still be edited or deleted later.

## Execution modes

### Execute immediately

Clicking the command sends it to the current terminal and runs it at once. Good for:

- Well-understood routine commands
- Daily inspection tasks
- Fixed-format read-only queries

### Append to prompt

Clicking the command only inserts it into the current input line without pressing Enter. Good for:

- Commands whose parameters still need checking
- Script fragments that usually need a small edit
- Higher-risk operations that should be reviewed manually first

## Variable prompts

Command scripts support `{{variableName}}` placeholders for dynamic parameters, for example:

```bash
docker exec -it {{container_name}} bash
```

When you run the command, NyaTerm opens a variable input dialog so the template can be completed before sending it.

## Categories, search, sorting, and view modes

The Quick Commands panel supports these management patterns:

- Search by label, command content, or description
- Filter by category from the dropdown
- Create, rename, move, and sort categories
- Keep pinned commands at the top
- Sort by modes such as name, usage frequency, or recently used
- Switch between compact and regular views to fit different panel widths
- Reuse existing categories when creating new commands; imports also try to preserve the category structure from the external file

Common ways to categorize:

- Kubernetes
- Docker
- Database
- Release scripts
- Environment inspection

## Export quick commands

Click **Export Commands** in the upper-right corner of the Quick Commands panel to export the current commands and categories as a NyaTerm JSON file. The exported file can be used to:

- Merging into another device with **Import Quick Commands**
- Sharing a team template
- Keeping a rollback copy before reorganizing many categories

The export does not include terminal history or other application settings. It only covers quick commands and their categories.

## Import quick commands

Click **Import Quick Commands** in the upper-right corner of the Quick Commands panel to import commands from an external file. Imports merge by command ID and update matching commands without clearing your existing list.

### Import from WindTerm

WindTerm quick commands are usually stored at:

```text
~/.wind/profiles/default.v10/terminal/quickbar.config
```

On Windows, look under your user directory:

```text
C:\Users\<username>\.wind\profiles\default.v10\terminal\quickbar.config
```

Choose **WindTerm Quickbar**, then select this `quickbar.config` file. NyaTerm reads fields such as `quick.label`, `quick.text`, `quick.group`, and `quick.uuid`. WindTerm commands with `quick.type` set to `Send Text` are imported as **Append to prompt** so imported scripts are not executed immediately when clicked.

### Import from a JSON file

Choose **NyaTerm JSON** to import a file like this:

```json
{
  "categories": [
    {
      "id": "general",
      "name": "General"
    },
    {
      "id": "k8s",
      "name": "Kubernetes"
    }
  ],
  "commands": [
    {
      "id": "cmd-list-files",
      "label": "List files",
      "command": "ls -la",
      "category_id": "general",
      "description": "List files with details",
      "color_tag": "blue",
      "icon_tag": "terminal",
      "pinned": true,
      "execution_mode": "execute",
      "source": "manual",
      "risk_level": "low"
    },
    {
      "label": "Kubernetes pods",
      "command": "kubectl get pods -A",
      "category": "Kubernetes",
      "execution_mode": "append",
      "risk_level": "low"
    }
  ]
}
```

Notes:

- `categories` is optional; when a command uses a `category` name, missing categories are created automatically
- `id` is optional and is generated when omitted
- `execution_mode` supports `execute` or `append`
- `source` supports `manual` or `ai`
- `risk_level` supports `low`, `medium`, `high`, or `critical`

## How it fits the workspace

Quick Commands are not tied to one specific session type. As long as the current terminal can accept input, you can send commands to:

- SSH sessions
- Local Terminal sessions
- Some serial workflows that need repeated fixed input

Common combinations include:

- Watching logs on one side while triggering diagnostics from Quick Commands on the other
- Running deploy commands remotely while building or using Git locally
- Turning variable-based commands into team-friendly templates
- Saving approved commands generated by AI Assistant as quick commands for later reuse
