"""Isolated native POSIX PTY checks; no user shell configuration is read."""
import errno
import os
from pathlib import Path
import select
import signal
import sys
import tempfile
import time


def check(shell, flags, files, wait_for_input=False, expect_cwd=True):
    with tempfile.TemporaryDirectory(prefix="zzclawterm-startup-test-") as root:
        root = Path(root)
        for name, contents in files.items():
            target = root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(contents)
            if name.startswith("bin/"):
                target.chmod(0o700)
        startup_dir = root / "startup"
        startup_dir.mkdir(mode=0o700)
        pid, fd = os.forkpty()
        if pid == 0:
            os.chdir(root)
            for key in list(os.environ):
                if key.startswith("ZZCLAWTERM_") or key in ("PROMPT_COMMAND", "BASH_ENV", "ENV", "ZDOTDIR"):
                    os.environ.pop(key)
            os.environ.update(HOME=str(root), TMPDIR=str(root), XDG_CONFIG_HOME=str(root / ".config"), TERM="xterm-256color")
            os.environ["PATH"] = str(root / "bin") + ":" + os.environ["PATH"]
            os.execv("/bin/sh", ["sh", "-c", sys.argv[1], "zzclawterm-test", str(startup_dir), "native", shell, *flags])
        data = b""
        sent = False
        exited = False
        deadline = time.monotonic() + 10
        try:
            while time.monotonic() < deadline:
                if select.select([fd], [], [], 0.05)[0]:
                    try:
                        chunk = os.read(fd, 65536)
                    except OSError as error:
                        if error.errno == errno.EIO:
                            break
                        raise
                    if not chunk:
                        break
                    data += chunk
                    if b"\x1b[6n" in chunk:
                        os.write(fd, b"\x1b[1;1R")
                if wait_for_input and not sent and b"ZZCLAWTERM_WAITING" in data:
                    # Longer than the old injection delay: no script may satisfy read.
                    time.sleep(0.7)
                    os.write(fd, b"user-input\r")
                    sent = True
                ready = (b"\x1b]7;file://localhost" in data) if expect_cwd else (b"ZZCLAWTERM_USER_PROMPT" in data)
                if ready and not exited:
                    os.write(fd, b"exit\r")
                    exited = True
            assert exited, f"{shell}: no expected prompt/CWD"
            if wait_for_input:
                assert b"ZZCLAWTERM_INPUT_OK" in data, f"{shell}: startup read did not receive user input"
            if expect_cwd:
                assert str(root).encode() in data, f"{shell}: wrong cwd"
            assert b"readonly variable" not in data, f"{shell}: readonly prompt was modified"
            if shell == "zsh":
                markers = [b"ZZCLAWTERM_ENV", b"ZZCLAWTERM_PROFILE", b"ZZCLAWTERM_RC", b"ZZCLAWTERM_LOGIN"]
                assert all(data.count(marker) == 1 for marker in markers), "zsh: rc files must run once"
                assert [data.index(marker) for marker in markers] == sorted(data.index(marker) for marker in markers), "zsh: wrong rc order"
            while True:
                waited, status = os.waitpid(pid, os.WNOHANG)
                if waited:
                    break
                assert time.monotonic() < deadline, f"{shell}: exit timed out"
                time.sleep(0.01)
            pid = None
            assert os.waitstatus_to_exitcode(status) == 0, f"{shell}: abnormal exit"
            assert startup_dir.is_dir(), f"{shell}: shell must not delete host-owned startup files"
        finally:
            os.close(fd)
            if pid is not None:
                try:
                    os.kill(pid, signal.SIGHUP)
                    os.waitpid(pid, 0)
                except ProcessLookupError:
                    pass


check("bash", ["--noprofile", "-i"], {
    ".bashrc": "printf 'ZZCLAWTERM_WAITING\\n'; IFS= read -r value; [ \"$value\" != user-input ] || printf 'ZZCLAWTERM_INPUT_OK\\n'\n"
}, wait_for_input=True)
check("bash", ["--noprofile", "-i"], {
    ".bashrc": "PROMPT_COMMAND=\"printf 'ZZCLAWTERM_USER_PROMPT\\\\n'\"\n"
}, expect_cwd=False)
check("bash", ["--noprofile", "-i"], {
    ".bashrc": "PS1=ZZCLAWTERM_USER_PROMPT; readonly PROMPT_COMMAND\n"
}, expect_cwd=False)
check("zsh", ["-l", "-i"], {
    ".zshenv": "printf 'ZZCLAWTERM_ENV\\n'\n",
    ".zprofile": "printf 'ZZCLAWTERM_PROFILE\\n'\n",
    ".zshrc": "printf 'ZZCLAWTERM_RC\\n'; printf 'ZZCLAWTERM_WAITING\\n'; read value; [[ $value != user-input ]] || printf 'ZZCLAWTERM_INPUT_OK\\n'\n",
    ".zlogin": "printf 'ZZCLAWTERM_LOGIN\\n'\n",
}, wait_for_input=True)
check("fish", ["-i"], {
    ".config/fish/config.fish": "printf 'ZZCLAWTERM_WAITING\\n'; read value; if test \"$value\" = user-input; printf 'ZZCLAWTERM_INPUT_OK\\n'; end\n"
}, wait_for_input=True)
check("", [], {
    "bin/getent": "#!/bin/sh\nprintf 'test:x:1000:1000:test:/unused:/usr/bin/fish\\n'\n",
    ".config/fish/config.fish": "printf 'ZZCLAWTERM_WAITING\\n'; read value; if test \"$value\" = user-input; printf 'ZZCLAWTERM_INPUT_OK\\n'; end\n"
}, wait_for_input=True)
print("Bash, Zsh and Fish startup, input, rc order and cleanup checks passed")
