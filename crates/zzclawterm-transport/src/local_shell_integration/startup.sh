# Executed as a command argument, never sent to the terminal's stdin.
zzclawterm_dir=$1
zzclawterm_namespace=$2
zzclawterm_shell=$3
shift 3
if [ -z "$zzclawterm_shell" ]; then
    zzclawterm_shell=$(getent passwd "$(id -u)" 2>/dev/null | cut -d: -f7)
    [ -n "$zzclawterm_shell" ] || zzclawterm_shell=${SHELL:-/bin/sh}
    set -- -l -i
fi
case "${zzclawterm_shell##*/}" in
    bash|bash.exe|zsh|zsh.exe|fish|fish.exe) ;;
    *) exec "$zzclawterm_shell" "$@" ;;
esac

# Rust owns this directory and removes it after the PTY and processes close.
# WSL must resolve the host path in the selected distribution; no guest /tmp
# directory or EXIT trap is needed, including when startup blocks in an rc file.
case $zzclawterm_namespace in
    wsl) zzclawterm_dir=$(wslpath -u "$zzclawterm_dir") || exec "$zzclawterm_shell" "$@" ;;
    native)
        case $zzclawterm_dir in
            [a-zA-Z]:/*) zzclawterm_dir=$(cygpath -u "$zzclawterm_dir") || exec "$zzclawterm_shell" "$@" ;;
        esac
        ;;
esac
ZZCLAWTERM_LOCAL_DIR=$zzclawterm_dir
export ZZCLAWTERM_LOCAL_DIR

cat > "$zzclawterm_dir/bash" <<'ZZCLAWTERM_BASH_EOF' || exec "$zzclawterm_shell" "$@"
# Remove only our loader, preserving string and array PROMPT_COMMAND hooks.
if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) =~ ^declare\ -[^[:space:]]*r ]]; then
    unset ZZCLAWTERM_LOCAL_DIR
    return 0
fi
__zzclawterm_loader='if [ -n "${ZZCLAWTERM_LOCAL_DIR:-}" ]; then . "$ZZCLAWTERM_LOCAL_DIR/bash"; fi'
for __zzclawterm_index in "${!PROMPT_COMMAND[@]}"; do
    PROMPT_COMMAND[$__zzclawterm_index]=${PROMPT_COMMAND[$__zzclawterm_index]//"$__zzclawterm_loader"/:}
done
unset __zzclawterm_loader __zzclawterm_index
__zzclawterm_local_cwd() {
    local ret=$?
    local path=$PWD
    # MSYS mounts such as /tmp need not be drive-letter aliases. Emit the
    # actual Windows URI rather than guessing their host mapping.
    case $OSTYPE in
        msys*|cygwin*) path=$(pwd -W 2>/dev/null) || path=$(cygpath -m "$PWD" 2>/dev/null) || path=$PWD ;;
    esac
    case $path in
        [a-zA-Z]:/*) path=/$path ;;
    esac
    printf '\033]7;file://localhost%s\007' "${path//%/%25}"
    return "$ret"
}
if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == 'declare -a '* ]]; then
    PROMPT_COMMAND+=(__zzclawterm_local_cwd)
else
    PROMPT_COMMAND="${PROMPT_COMMAND:+$PROMPT_COMMAND
}__zzclawterm_local_cwd"
fi
export -n PROMPT_COMMAND
unset ZZCLAWTERM_LOCAL_DIR
__zzclawterm_local_cwd
ZZCLAWTERM_BASH_EOF

cat > "$zzclawterm_dir/fish" <<'ZZCLAWTERM_FISH_EOF' || exec "$zzclawterm_shell" "$@"
function __zzclawterm_local_cwd --on-event fish_prompt
    printf '\033]7;file://localhost%s\007' (string replace -a -- '%' '%25' $PWD)
end
set -e ZZCLAWTERM_LOCAL_DIR
ZZCLAWTERM_FISH_EOF

cat > "$zzclawterm_dir/.zshenv" <<'ZZCLAWTERM_ZSHENV_EOF' || exec "$zzclawterm_shell" "$@"
if [[ $ZZCLAWTERM_LOCAL_USER_ZDOTDIR_SET = x ]]; then
    ZDOTDIR=$ZZCLAWTERM_LOCAL_USER_ZDOTDIR
else
    unset ZDOTDIR
fi
[[ ! -r "${ZDOTDIR:-$HOME}/.zshenv" ]] || source "${ZDOTDIR:-$HOME}/.zshenv"
ZZCLAWTERM_LOCAL_USER_ZDOTDIR=${ZDOTDIR:-$HOME}
ZZCLAWTERM_LOCAL_USER_ZDOTDIR_SET=${ZDOTDIR+x}
if [[ -o rcs ]]; then
    ZDOTDIR=$ZZCLAWTERM_LOCAL_DIR
else
    unset ZZCLAWTERM_LOCAL_DIR ZZCLAWTERM_LOCAL_USER_ZDOTDIR ZZCLAWTERM_LOCAL_USER_ZDOTDIR_SET
fi
ZZCLAWTERM_ZSHENV_EOF

cat > "$zzclawterm_dir/.zprofile" <<'ZZCLAWTERM_ZPROFILE_EOF' || exec "$zzclawterm_shell" "$@"
if [[ $ZZCLAWTERM_LOCAL_USER_ZDOTDIR_SET = x ]]; then
    ZDOTDIR=$ZZCLAWTERM_LOCAL_USER_ZDOTDIR
else
    unset ZDOTDIR
fi
[[ ! -r "${ZDOTDIR:-$HOME}/.zprofile" ]] || source "${ZDOTDIR:-$HOME}/.zprofile"
ZZCLAWTERM_LOCAL_USER_ZDOTDIR=${ZDOTDIR:-$HOME}
ZZCLAWTERM_LOCAL_USER_ZDOTDIR_SET=${ZDOTDIR+x}
if [[ -o rcs ]]; then
    ZDOTDIR=$ZZCLAWTERM_LOCAL_DIR
else
    unset ZZCLAWTERM_LOCAL_DIR ZZCLAWTERM_LOCAL_USER_ZDOTDIR ZZCLAWTERM_LOCAL_USER_ZDOTDIR_SET
fi
ZZCLAWTERM_ZPROFILE_EOF

cat > "$zzclawterm_dir/.zshrc" <<'ZZCLAWTERM_ZSHRC_EOF' || exec "$zzclawterm_shell" "$@"
if [[ $ZZCLAWTERM_LOCAL_USER_ZDOTDIR_SET = x ]]; then
    ZDOTDIR=$ZZCLAWTERM_LOCAL_USER_ZDOTDIR
else
    unset ZDOTDIR
fi
[[ ! -r "${ZDOTDIR:-$HOME}/.zshrc" ]] || source "${ZDOTDIR:-$HOME}/.zshrc"
__zzclawterm_local_cwd() {
    printf '\033]7;file://localhost%s\007' "${PWD//%/%25}"
}
autoload -Uz add-zsh-hook
add-zsh-hook precmd __zzclawterm_local_cwd
if [[ -o login && -o rcs ]]; then
    ZZCLAWTERM_LOCAL_USER_ZDOTDIR=${ZDOTDIR:-$HOME}
    ZZCLAWTERM_LOCAL_USER_ZDOTDIR_SET=${ZDOTDIR+x}
    ZDOTDIR=$ZZCLAWTERM_LOCAL_DIR
else
    unset ZZCLAWTERM_LOCAL_DIR ZZCLAWTERM_LOCAL_USER_ZDOTDIR ZZCLAWTERM_LOCAL_USER_ZDOTDIR_SET
fi
ZZCLAWTERM_ZSHRC_EOF

cat > "$zzclawterm_dir/.zlogin" <<'ZZCLAWTERM_ZLOGIN_EOF' || exec "$zzclawterm_shell" "$@"
if [[ $ZZCLAWTERM_LOCAL_USER_ZDOTDIR_SET = x ]]; then
    ZDOTDIR=$ZZCLAWTERM_LOCAL_USER_ZDOTDIR
else
    unset ZDOTDIR
fi
[[ ! -r "${ZDOTDIR:-$HOME}/.zlogin" ]] || source "${ZDOTDIR:-$HOME}/.zlogin"
unset ZZCLAWTERM_LOCAL_DIR ZZCLAWTERM_LOCAL_USER_ZDOTDIR ZZCLAWTERM_LOCAL_USER_ZDOTDIR_SET
ZZCLAWTERM_ZLOGIN_EOF

case "${zzclawterm_shell##*/}" in
    bash|bash.exe)
        # Profiles retain control: if they replace PROMPT_COMMAND, integration
        # is skipped rather than overriding user startup behavior.
        PROMPT_COMMAND='if [ -n "${ZZCLAWTERM_LOCAL_DIR:-}" ]; then . "$ZZCLAWTERM_LOCAL_DIR/bash"; fi'${PROMPT_COMMAND:+"; $PROMPT_COMMAND"}
        export PROMPT_COMMAND
        exec "$zzclawterm_shell" "$@"
        ;;
    zsh|zsh.exe)
        ZZCLAWTERM_LOCAL_USER_ZDOTDIR=${ZDOTDIR:-$HOME}
        ZZCLAWTERM_LOCAL_USER_ZDOTDIR_SET=${ZDOTDIR+x}
        ZDOTDIR=$zzclawterm_dir
        export ZZCLAWTERM_LOCAL_USER_ZDOTDIR ZZCLAWTERM_LOCAL_USER_ZDOTDIR_SET ZDOTDIR
        exec "$zzclawterm_shell" "$@"
        ;;
    fish|fish.exe)
        exec "$zzclawterm_shell" --init-command 'source "$ZZCLAWTERM_LOCAL_DIR/fish"' "$@"
        ;;
esac
