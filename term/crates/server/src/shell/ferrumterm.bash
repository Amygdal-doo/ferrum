# FerrumTerm shell integration for bash (4.4 or later): marks each prompt
# and command (OSC 133) so FerrumTerm can group a command with its output,
# show its exit code, copy and re-run it. Add it at the end of ~/.bashrc:
#   eval "$(fterm shell-integration bash)"
[[ $- == *i* ]] || return 0
[[ -n "${_FTERM_SI_LOADED-}" ]] && return 0
_FTERM_SI_LOADED=1
# PS0 (shown when a command starts) needs bash 4.4.
(( BASH_VERSINFO[0] > 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] >= 4) )) || return 0

# Percent-encodes $1 (bytes outside [A-Za-z0-9._~-]).
_fterm_si_url() {
  local LC_ALL=C s=$1 out= c i
  for (( i = 0; i < ${#s}; i++ )); do
    c=${s:i:1}
    case $c in
      [a-zA-Z0-9._~-]) out+=$c ;;
      *) printf -v c '%%%02X' "'$c"; out+=$c ;;
    esac
  done
  printf '%s' "$out"
}

# Before each prompt: the last command's exit code (ignored when no
# command ran), then "a prompt starts".
_fterm_si_prompt() {
  local ret=$?
  printf '\e]133;D;%d\a\e]133;A\a' "$ret"
  return $ret
}

# When a command starts (PS0, in a subshell): "its output starts".
_fterm_si_ps0() {
  local line
  line=$(HISTTIMEFORMAT= builtin history 1)
  line=${line#*[0-9] }
  printf '\e]133;C;cmdline_url=%s\a' "$(_fterm_si_url "${line# }")"
}

PROMPT_COMMAND="_fterm_si_prompt${PROMPT_COMMAND:+; $PROMPT_COMMAND}"
PS0="${PS0-}\$(_fterm_si_ps0)"
