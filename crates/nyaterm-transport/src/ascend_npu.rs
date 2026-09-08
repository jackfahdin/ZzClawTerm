use std::time::Duration;

use serde::Serialize;

use crate::{
    RemoteCommandOutput, SshMultiplexHandle, SshSessionConfig, ensure_remote_command_success,
    run_ssh_command,
};

const ASCEND_NPU_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct RemoteNpuOverview {
    pub available: bool,
    pub driver_version: String,
    pub cann_version: String,
    pub npus: Vec<RemoteNpu>,
    pub processes: Vec<RemoteNpuProcess>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RemoteNpu {
    pub index: u32,
    pub chip_id: u32,
    pub physical_id: Option<u32>,
    pub device_key: String,
    pub name: String,
    pub health: String,
    pub bus_id: String,
    pub temperature_c: Option<f64>,
    pub utilization_aicore_percent: Option<f64>,
    pub utilization_memory_percent: Option<f64>,
    pub memory_total_mb: u64,
    pub memory_used_mb: u64,
    pub memory_free_mb: u64,
    pub memory_kind: String,
    pub hbm_total_mb: Option<u64>,
    pub hbm_used_mb: Option<u64>,
    pub power_draw_w: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RemoteNpuProcess {
    pub npu_index: u32,
    pub chip_id: u32,
    pub device_key: String,
    pub pid: u32,
    pub process_name: String,
    pub used_memory_mb: u64,
}

#[derive(Debug, Clone)]
pub struct RemoteNpuService {
    config: SshSessionConfig,
    multiplex: Option<SshMultiplexHandle>,
}

pub const ASCEND_NPU_OVERVIEW_SCRIPT: &str = r#"sh -s <<'NYATERM_ASCEND_NPU_SCRIPT'
LC_ALL=C
export LC_ALL

find_npu_smi() {
  if command -v npu-smi >/dev/null 2>&1; then
    command -v npu-smi
    return 0
  fi

  for candidate in \
    /usr/local/bin/npu-smi \
    /usr/local/Ascend/driver/tools/npu-smi \
    /usr/local/Ascend/driver/tools/*/npu-smi
  do
    if [ -x "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  return 1
}

read_install_version() {
  file=$1
  awk -F= '
    {
      key=$1
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", key)
      if (tolower(key) == "version") {
        value=$0
        sub(/^[^=]*=/, "", value)
        gsub(/^[[:space:]"]+|[[:space:]"]+$/, "", value)
        print value
        exit
      }
    }
  ' "$file" 2>/dev/null
}

find_cann_version() {
  for root in "${ASCEND_HOME_PATH:-}" /usr/local/Ascend "${HOME:-}/Ascend"; do
    [ -n "$root" ] || continue
    for info_file in \
      "$root"/ascend-toolkit/latest/*-linux/ascend_toolkit_install.info \
      "$root"/nnae/latest/ascend_nnae_install.info \
      "$root"/nnrt/latest/*-linux/ascend_nnrt_install.info
    do
      [ -r "$info_file" ] || continue
      version=$(read_install_version "$info_file")
      if [ -n "$version" ]; then
        printf '%s\n' "$version"
        return 0
      fi
    done
  done
  return 1
}

npu_smi=$(find_npu_smi 2>/dev/null || true)
if [ -z "$npu_smi" ]; then
  printf 'NPU_AVAILABLE\t0\n'
  exit 0
fi

npu_output=$("$npu_smi" info 2>&1)
status=$?
if [ "$status" -ne 0 ] || [ -z "$npu_output" ]; then
  printf 'NPU_AVAILABLE\t0\n'
  printf 'NPU_ERROR\t%s\n' "$(printf '%s' "$npu_output" | tr '\n\t' '  ' | cut -c1-500)"
  exit 0
fi

printf 'NPU_AVAILABLE\t1\n'
cann_version=$(find_cann_version 2>/dev/null || true)
printf 'NPU_CANN_VERSION\t%s\n' "$cann_version"
printf 'NPU_SMI_BEGIN\n'
printf '%s\n' "$npu_output"
printf 'NPU_SMI_END\n'
NYATERM_ASCEND_NPU_SCRIPT
"#;

impl RemoteNpuService {
    pub fn new(config: SshSessionConfig) -> Self {
        Self {
            config,
            multiplex: None,
        }
    }

    pub fn with_multiplex(
        config: SshSessionConfig,
        multiplex: SshMultiplexHandle,
    ) -> anyhow::Result<Self> {
        multiplex.ensure_matches_config(&config)?;
        Ok(Self {
            config,
            multiplex: Some(multiplex),
        })
    }

    pub fn overview(&self) -> anyhow::Result<RemoteNpuOverview> {
        let output = self.exec_success(
            ASCEND_NPU_OVERVIEW_SCRIPT,
            "Failed to fetch Ascend NPU overview",
        )?;
        Ok(parse_npu_overview_output(&output.stdout))
    }

    fn exec_success(&self, command: &str, context: &str) -> anyhow::Result<RemoteCommandOutput> {
        let output = run_ssh_command(
            self.config.clone(),
            self.multiplex.clone(),
            command.as_bytes().to_vec(),
            ASCEND_NPU_TIMEOUT,
        )?;
        ensure_remote_command_success(output, context)
    }
}

pub fn parse_npu_overview_output(output: &str) -> RemoteNpuOverview {
    let mut overview = RemoteNpuOverview::default();
    let mut in_smi = false;
    let mut raw_lines = Vec::new();

    for line in output.lines() {
        match line {
            "NPU_SMI_BEGIN" => {
                in_smi = true;
                continue;
            }
            "NPU_SMI_END" => {
                in_smi = false;
                continue;
            }
            _ => {}
        }

        if in_smi {
            raw_lines.push(line.to_string());
            continue;
        }

        let cols: Vec<&str> = line.split('\t').collect();
        if cols.first() == Some(&"NPU_AVAILABLE") && cols.len() >= 2 {
            overview.available = cols[1].trim() == "1";
        } else if cols.first() == Some(&"NPU_CANN_VERSION") && cols.len() >= 2 {
            overview.cann_version = cols[1].trim().to_string();
        }
    }

    if !overview.available {
        return overview;
    }

    parse_npu_smi_lines(&raw_lines, &mut overview);
    overview
}

fn parse_npu_smi_lines(lines: &[String], overview: &mut RemoteNpuOverview) {
    let mut in_processes = false;
    let mut current: Option<RemoteNpu> = None;

    for line in lines {
        if overview.driver_version.is_empty()
            && let Some(version) = parse_driver_version(line)
        {
            overview.driver_version = version;
        }

        let cells = parse_table_cells(line);
        if cells.is_empty() {
            continue;
        }
        let lower = cells.join(" ").to_ascii_lowercase();
        if lower.contains("process") && lower.contains("pid") {
            if let Some(npu) = current.take() {
                overview.npus.push(npu);
            }
            in_processes = true;
            continue;
        }
        if lower.contains("npu") && lower.contains("chip") && lower.contains("health") {
            in_processes = false;
            continue;
        }

        if in_processes {
            if let Some(process) = parse_process_cells(&cells) {
                overview.processes.push(process);
            }
            continue;
        }

        if let Some(mut npu) = parse_device_cells(&cells) {
            if let Some(existing) = current.take() {
                overview.npus.push(existing);
            }
            npu.device_key = format!("{}:{}", npu.index, npu.chip_id);
            current = Some(npu);
        } else if let Some(npu) = current.as_mut() {
            apply_metric_cells(npu, &cells);
        }
    }

    if let Some(npu) = current {
        overview.npus.push(npu);
    }
}

fn parse_driver_version(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    if !lower.contains("version") && !lower.contains("npu-smi") {
        return None;
    }
    line.split_whitespace()
        .find(|part| part.chars().any(|ch| ch.is_ascii_digit()) && part.contains('.'))
        .map(|part| {
            part.trim_matches(|ch: char| {
                !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
            })
            .to_string()
        })
}

fn parse_table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if !trimmed.contains('|') {
        return Vec::new();
    }
    trimmed
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .filter(|cell| !cell.is_empty() && !cell.chars().all(|ch| matches!(ch, '-' | '+')))
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_device_cells(cells: &[String]) -> Option<RemoteNpu> {
    if cells.len() < 4 {
        return None;
    }
    let index = parse_u32(&cells[0])?;
    let chip_id = parse_u32(&cells[1]).unwrap_or(0);
    Some(RemoteNpu {
        index,
        chip_id,
        physical_id: None,
        device_key: String::new(),
        name: cells.get(2).cloned().unwrap_or_default(),
        health: cells.get(3).cloned().unwrap_or_default(),
        bus_id: cells.get(4).cloned().unwrap_or_default(),
        temperature_c: cells.iter().find_map(|cell| parse_temperature(cell)),
        utilization_aicore_percent: None,
        utilization_memory_percent: None,
        memory_total_mb: 0,
        memory_used_mb: 0,
        memory_free_mb: 0,
        memory_kind: "memory".to_string(),
        hbm_total_mb: None,
        hbm_used_mb: None,
        power_draw_w: cells.iter().find_map(|cell| parse_watts(cell)),
    })
}

fn apply_metric_cells(npu: &mut RemoteNpu, cells: &[String]) {
    let joined = cells.join(" ");
    let lower = joined.to_ascii_lowercase();
    if lower.contains("aicore") {
        npu.utilization_aicore_percent = first_percent(&joined);
    }
    if lower.contains("memory") {
        npu.utilization_memory_percent = first_percent(&joined);
        if let Some((used, total)) = parse_used_total_mb(&joined) {
            npu.memory_used_mb = used;
            npu.memory_total_mb = total;
            npu.memory_free_mb = total.saturating_sub(used);
        }
    }
    if lower.contains("hbm")
        && let Some((used, total)) = parse_used_total_mb(&joined)
    {
        npu.hbm_used_mb = Some(used);
        npu.hbm_total_mb = Some(total);
        npu.memory_kind = "hbm".to_string();
        npu.memory_used_mb = used;
        npu.memory_total_mb = total;
        npu.memory_free_mb = total.saturating_sub(used);
    }
}

fn parse_process_cells(cells: &[String]) -> Option<RemoteNpuProcess> {
    let npu_index = cells.iter().find_map(|cell| parse_u32(cell))?;
    let pid = cells.iter().skip(1).find_map(|cell| parse_u32(cell))?;
    let used_memory_mb = cells
        .iter()
        .rev()
        .find_map(|cell| parse_u64(cell))
        .unwrap_or(0);
    let process_name = cells
        .iter()
        .find(|cell| cell.chars().any(|ch| ch.is_ascii_alphabetic()))
        .cloned()
        .unwrap_or_default();
    Some(RemoteNpuProcess {
        npu_index,
        chip_id: 0,
        device_key: format!("{npu_index}:0"),
        pid,
        process_name,
        used_memory_mb,
    })
}

fn parse_u32(value: &str) -> Option<u32> {
    value
        .split(|ch: char| !ch.is_ascii_digit())
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse().ok())
}

fn parse_u64(value: &str) -> Option<u64> {
    value
        .split(|ch: char| !ch.is_ascii_digit())
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse().ok())
}

fn first_percent(value: &str) -> Option<f64> {
    value
        .split_whitespace()
        .find_map(|part| part.trim_end_matches('%').parse::<f64>().ok())
}

fn parse_temperature(value: &str) -> Option<f64> {
    let lower = value.to_ascii_lowercase();
    lower.contains('c').then(|| first_number(value)).flatten()
}

fn parse_watts(value: &str) -> Option<f64> {
    let lower = value.to_ascii_lowercase();
    lower.contains('w').then(|| first_number(value)).flatten()
}

fn first_number(value: &str) -> Option<f64> {
    value
        .split(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse().ok())
}

fn parse_used_total_mb(value: &str) -> Option<(u64, u64)> {
    let numbers = value
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect::<Vec<_>>();
    match numbers.as_slice() {
        [used, total, ..] => Some((*used, *total)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_npu_overview_output;

    #[test]
    fn parses_npu_unavailable() {
        let overview = parse_npu_overview_output("NPU_AVAILABLE\t0\n");

        assert!(!overview.available);
        assert!(overview.npus.is_empty());
    }

    #[test]
    fn parses_simple_npu_table() {
        let output = "NPU_AVAILABLE\t1\nNPU_CANN_VERSION\t8.0\nNPU_SMI_BEGIN\n| npu-smi 25.2.0 Version: 25.2.0 |\n| NPU | Chip | Name | Health | Bus-Id |\n| 0 | 0 | Ascend 910B | OK | 0000:01:00.0 |\n| AICore | 31% |\n| HBM-Usage | 1024 / 32768 MB |\nNPU_SMI_END\n";
        let overview = parse_npu_overview_output(output);

        assert!(overview.available);
        assert_eq!(overview.driver_version, "25.2.0");
        assert_eq!(overview.cann_version, "8.0");
        assert_eq!(overview.npus.len(), 1);
        assert_eq!(overview.npus[0].memory_total_mb, 32768);
    }
}
