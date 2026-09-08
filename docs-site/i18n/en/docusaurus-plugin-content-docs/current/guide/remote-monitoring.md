# Remote Host Monitoring

NyaTerm provides five right-side monitoring panels for SSH sessions: **Resource Monitor**, **NVIDIA GPU Monitor**, **Ascend NPU Monitor**, **Process Manager**, and **Docker Manager**. They share some behavior:

- They only make sense for an **SSH session**, and bind only to a genuinely active SSH session
- Each is shown or hidden by its own toggle in **Settings → Terminal**; turning a toggle off also hides its activity-bar icon
- GPU, NPU, process, and Docker panels accept **3 to 120 seconds**; the resource-monitor panel separately accepts **1 to 60 seconds**
- A panel stops refreshing after several consecutive polling failures, to avoid repeatedly hitting an unsupported host

Default toggle states are below. Note that Process Manager and Docker Manager are **on by default**:

| Panel | Setting | Default | Default interval |
|-------|---------|---------|------------------|
| Resource Monitor | Show remote resource info | On | 3 s |
| NVIDIA GPU Monitor | Show NVIDIA GPU Monitor | Off | 3 s |
| Ascend NPU Monitor | Show Ascend NPU Monitor | Off | 3 s |
| Process Manager | Show Process Manager | On | 5 s |
| Docker Manager | Show Docker Manager | On | 10 s |

## Resource Monitor

Remote resource monitoring is on by default. To see data, both of these must be true:

1. The current tab is an **SSH session**
2. **Show Remote Resource Stats** is enabled in **Settings → Terminal**

When enabled, the **Resource Monitor** icon appears in the right activity bar and the panel polls the host on the configured interval. The default interval is **3 seconds**, and you can change it manually.

The panel displays:

- Hostname, OS, architecture, uptime
- Load average
- CPU usage
- Memory usage
- Network throughput

## NVIDIA GPU Monitor

The **NVIDIA GPU Monitor** panel shows NVIDIA GPU status on the remote host. It is off by default; enable **Show NVIDIA GPU Monitor** in **Settings → Terminal**.

The panel displays:

- Driver version and CUDA version
- Summary: GPU count, highest utilization, memory usage, highest temperature
- Per-GPU card: index, model, performance state (pstate), utilization and memory bars; expand for UUID, temperature, power draw, fan speed, and free memory. Utilization above **70% / 90%** is color-coded differently
- A searchable GPU process list (filter by PID, GPU index, user, or process name), sorted by GPU memory used

If the remote host has no NVIDIA GPU or is missing `nvidia-smi`, the panel shows a matching empty state.

## Ascend NPU Monitor

The **Ascend NPU Monitor** panel shows Ascend NPU status on the remote host. It is off by default; enable **Show Ascend NPU Monitor** in **Settings → Terminal**.

The panel displays:

- Driver version and CANN version
- Summary: NPU count, highest AI Core utilization, memory usage, highest temperature
- Per device: device index, Physical ID, Bus ID, AI Core utilization, memory usage, temperature, and power draw
- A searchable NPU compute process list

If the remote host returns no Ascend NPU information, the panel shows a matching empty state.

## Process Manager

The **Process Manager** panel shows a live process list from the remote host. It is **on by default**; turn off **Show Process Manager** in **Settings → Terminal** to hide it.

Key capabilities:

- Total process count and a search box (filter by PID, user, state, command, or full command line)
- Adaptive layout that adds or drops columns based on panel width; sort by process name, PID, CPU%, MEM%, or user
- Expand a process for PID/PPID, user, state, CPU%, memory%, RSS, elapsed time, and the full command line, plus adjusting the nice value (`-20` to `19`) and clicking **Apply** (renice)
- A row action menu to copy the PID or command, or send `TERM` / `HUP` / `STOP` / `CONT` signals; `KILL` first opens a confirmation dialog showing the `kill` command

If the remote host does not support process queries, the panel shows a distinct message.

## Docker Manager

The **Docker Manager** panel manages Docker on the remote host. It is **on by default**; turn off **Show Docker Manager** in **Settings → Terminal** to hide it.

Key capabilities:

- Overview: running / stopped container counts and image count, with the Docker engine version in the header
- Global search plus tabs for containers, images, volumes, networks, and Compose (when available); extra tabs collapse into a **More** dropdown
- **Containers**: a state-sorted virtualized list; a row menu views logs (runs `docker logs -f` in the terminal), enters the container (opens a shell), starts / stops / restarts / kills (confirm) / removes (confirm); clicking a row opens a live-refreshing details dialog
- **Images / Volumes / Networks**: fetched on demand, each row supports removal (confirm)
- **Compose**: lists projects; expand to lazily load services; supports project-level up / restart / down and service-level logs / enter / up / stop / restart
- The **More** menu offers `docker system prune` (destructive, confirmed)

Logs and enter-container actions run in the real terminal session; remove, kill, Compose down, prune, and other destructive operations route through a confirmation dialog showing the exact command.
