"""Build once and collect the ZMODEM matrix as JSONL (temporary loopback data only)."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import time


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bytes', type=int, default=8 * 1024 * 1024)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--rtt-ms', type=int, default=50)
    parser.add_argument('--delay-ms', type=int, default=2)
    parser.add_argument('--rate-kib', type=int, default=4096)
    parser.add_argument('--output', type=Path, default=Path('zmodem-results.jsonl'))
    parser.add_argument('--release', action='store_true')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    build = ['cargo', 'build', '-j', '4', '-p', 'nyaterm-transport', '--example', 'zmodem_bench']
    if args.release:
        build.append('--release')
    subprocess.run(build, cwd=root, check=True)
    target = Path(os.environ.get('CARGO_TARGET_DIR', root / 'target'))
    binary = target / ('release' if args.release else 'debug') / 'examples' / ('zmodem_bench.exe' if os.name == 'nt' else 'zmodem_bench')
    base = ['--bytes', str(args.bytes), '--rtt-ms', str(args.rtt_ms)]
    matrix = [
        ['--label', 'conservative', '--window', '10', '--receive', 'conservative', '--buffered', '0'],
        ['--label', 'bounded-path-buffered', '--window', '10', '--receive', 'conservative', '--buffered', '1'],
    ]
    matrix += [['--label', 'nonstop-window', '--window', str(window), '--receive', 'nonstop', '--buffered', '0'] for window in (10, 64, 256, 1024)]
    matrix += [['--label', 'nonstop-buffered', '--window', str(window), '--receive', 'nonstop', '--buffered', '1'] for window in (10, 64, 256, 1024)]
    for direction in ('upload', 'download'):
        for receive in ('conservative', 'nonstop'):
            for window in (10, 64, 256, 1024):
                matrix.append(['--label', 'runtime-loopback', '--layer', 'runtime', '--direction', direction, '--window', str(window), '--receive', receive])
        matrix.append(['--label', 'runtime-slow-peer', '--layer', 'runtime', '--direction', direction, '--delay-ms', str(args.delay_ms), '--rate-kib', str(args.rate_kib)])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    metadata = {'record': 'environment', 'platform': platform.platform(), 'profile': 'release' if args.release else 'debug', 'revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(), 'worktree_modified': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=root)), 'unix_time': time.time()}
    with args.output.open('w', encoding='utf-8') as output:
        output.write(json.dumps(metadata) + '\n')
        for repeat in range(args.repeats):
            for config in matrix:
                try:
                    completed = subprocess.run([str(binary), *base, *config], cwd=root, capture_output=True, text=True, timeout=180)
                    if completed.returncode:
                        result = {'success': False, 'config': config, 'error': completed.stderr.strip()}
                    else:
                        result = json.loads(completed.stdout)
                except subprocess.TimeoutExpired:
                    result = {'success': False, 'config': config, 'error': 'benchmark exceeded 180 seconds'}
                except (OSError, json.JSONDecodeError) as error:
                    result = {'success': False, 'config': config, 'error': str(error)}
                result['repeat'] = repeat
                output.write(json.dumps(result) + '\n')
                output.flush()
                print(json.dumps(result), flush=True)
                if not result['success']:
                    raise SystemExit('benchmark failed; result preserved in JSONL')


if __name__ == '__main__':
    main()
