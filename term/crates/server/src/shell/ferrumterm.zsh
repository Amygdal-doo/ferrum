# FerrumTerm shell integration for zsh: marks each prompt and command
# (OSC 133) so FerrumTerm can group a command with its output, show its
# exit code, copy and re-run it. FerrumTerm loads it by itself; elsewhere:
#   eval "$(fterm shell-integration zsh)"   # at the end of ~/.zshrc
[[ -o interactive ]] || return 0
[[ -n "${_FTERM_SI_LOADED-}" ]] && return 0
typeset -g _FTERM_SI_LOADED=1
typeset -gi _fterm_si_running=0

# Percent-encodes $1 (bytes outside [A-Za-z0-9._~-]).
_fterm_si_url() {
  emulate -L zsh
  local LC_ALL=C s=$1 out= c hex
  local -i i
  for (( i = 1; i <= ${#s}; i++ )); do
    c=${s[i]}
    case $c in
      [a-zA-Z0-9._~-]) out+=$c ;;
      *) printf -v hex '%%%02X' "'$c"; out+=$hex ;;
    esac
  done
  print -rn -- "$out"
}

# Before each prompt: the last command's exit code, then "a prompt starts".
_fterm_si_precmd() {
  local -i ret=$?
  if (( _fterm_si_running )); then
    printf '\e]133;D;%d\a' "$ret"
    _fterm_si_running=0
  fi
  printf '\e]133;A\a'
}

# Before running a command: "its output starts", with the command line.
_fterm_si_preexec() {
  _fterm_si_running=1
  printf '\e]133;C;cmdline_url=%s\a' "$(_fterm_si_url "$1")"
}

autoload -Uz add-zsh-hook
add-zsh-hook precmd _fterm_si_precmd
add-zsh-hook preexec _fterm_si_preexec
