# FerrumTerm shell integration for fish: marks each prompt and command
# (OSC 133) so FerrumTerm can group a command with its output, show its
# exit code, copy and re-run it. FerrumTerm loads it by itself (as a
# vendor_conf.d script); elsewhere, in ~/.config/fish/config.fish:
#   fterm shell-integration fish | source

# Put back the XDG_DATA_DIRS FerrumTerm changed to load this file.
if set -q FTERM_FISH_XDG_DATA_DIRS
    if test -n "$FTERM_FISH_XDG_DATA_DIRS"
        set -gx XDG_DATA_DIRS $FTERM_FISH_XDG_DATA_DIRS
    else
        set -e XDG_DATA_DIRS
    end
    set -e FTERM_FISH_XDG_DATA_DIRS
end

status is-interactive; or exit 0
set -q _fterm_si_loaded; and exit 0
set -g _fterm_si_loaded 1
# fish 4 marks prompts and commands itself.
test (string split -f1 . $version) -ge 4; and exit 0

function _fterm_si_prompt --on-event fish_prompt
    printf '\e]133;A\a'
end

function _fterm_si_preexec --on-event fish_preexec
    printf '\e]133;C;cmdline_url=%s\a' (string escape --style=url -- $argv[1])
end

function _fterm_si_postexec --on-event fish_postexec
    printf '\e]133;D;%d\a' $status
end
