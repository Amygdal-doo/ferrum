# Per-app metadata shared by the packaging scripts. Source it, then call
#   ferrum_app <app-id>
# which sets: APP_NAME, APP_FILE_NAME (no spaces), APP_BINARIES, DEB_PACKAGE,
# APP_SUMMARY and APP_WINDOWS (yes/no). Returns 1 for unknown ids.
# Keep in sync with scripts/bundle-macos.sh, scripts/package-windows.ps1,
# launcher/crates/core/src/catalog.rs and catalog.json.

ferrum_app() {
  APP_WINDOWS=yes
  case "$1" in
    api)
      APP_NAME="FerrumAPI"; APP_BINARIES="ferrumapi"; DEB_PACKAGE="ferrumapi"
      APP_SUMMARY="Native HTTP/API client: collections, environments and history" ;;
    git)
      APP_NAME="FerrumGIT"; APP_BINARIES="ferrumgit"; DEB_PACKAGE="ferrumgit"
      APP_SUMMARY="Native Git client with commit graph, staging and diffs" ;;
    data)
      APP_NAME="FerrumData"; APP_BINARIES="ferrumdata"; DEB_PACKAGE="ferrumdata"
      APP_SUMMARY="Local-first database IDE: SQL editor and result grid" ;;
    term)
      APP_NAME="FerrumTerm"; APP_BINARIES="ferrumterm fterm"; DEB_PACKAGE="ferrumterm"
      APP_SUMMARY="GPU terminal with persistent server sessions"
      # The server and client talk over Unix sockets; no Windows build yet.
      APP_WINDOWS=no ;;
    kit)
      APP_NAME="FerrumKit"; APP_BINARIES="ferrumkit"; DEB_PACKAGE="ferrumkit"
      APP_SUMMARY="Developer utilities: JSON, Base64, JWT, regex, hashes and more" ;;
    dock)
      APP_NAME="FerrumDock"; APP_BINARIES="ferrumdock"; DEB_PACKAGE="ferrumdock"
      APP_SUMMARY="Native container manager for Docker-compatible engines" ;;
    editor)
      APP_NAME="Ferrum Editor"; APP_BINARIES="ferrumeditor"; DEB_PACKAGE="ferrum-editor"
      APP_SUMMARY="Lightweight, hackable text editor: tree view, tabs, command palette" ;;
    ports)
      APP_NAME="FerrumPorts"; APP_BINARIES="ferrumports"; DEB_PACKAGE="ferrumports"
      APP_SUMMARY="Listening ports and the processes behind them" ;;
    env)
      APP_NAME="FerrumEnv"; APP_BINARIES="ferrumenv"; DEB_PACKAGE="ferrumenv"
      APP_SUMMARY="Password-locked vault for project environment variables" ;;
    launcher)
      APP_NAME="Ferrum Launcher"; APP_BINARIES="ferrum-launcher"; DEB_PACKAGE="ferrum-launcher"
      APP_SUMMARY="Installs, updates and rolls back the Ferrum apps" ;;
    *) return 1 ;;
  esac
  APP_FILE_NAME="${APP_NAME// /}"
}
