#!/usr/bin/env bash
# Dawnload Installer
# Builds Dawnload.app and installs everything you need — no terminal skills required,
# everything after this line is handled for you.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="Dawnload"
APP_ATLAS="/Applications/${APP_NAME}.app"

notice()  { printf "\n\033[1;36m==> %s\033[0m\n" "$*"; }
info()    { printf "\033[0;37m    %s\033[0m\n" "$*"; }
warn()    { printf "\033[1;33m!! %s\033[0m\n" "$*"; }
ok()      { printf "\033[1;32mOK %s\033[0m\n" "$*"; }

banner() {
cat <<'EOM'

    ╔══════════════════════════════════════════════════╗
    ║  Dawnload installer                              ║
    ║  one command. zero fuss.                          ║
    ╚══════════════════════════════════════════════════╝
EOM
}

main() {
  banner
  check_macos
  ensure_command_line_tools
  ensure_brew
  ensure_tool "yt-dlp"     "yt-dlp"
  ensure_tool "gallery-dl" "gallery-dl"
  ensure_tool "ffmpeg"     "ffmpeg"

  notice "All three download engines are ready. Building the app..."

  (cd "$ROOT" && ./Scripts/package-app.sh)

  if [[ ! -d "$ROOT/.build/${APP_NAME}.app" ]]; then
    die "Build finished but Dawnload.app was not found. Something went wrong."
  fi

  install_to_applications

  notice "Done! Open Dawnload and paste your first URL."
  info "Tip: use Cmd+D in the app to start a download from anywhere."
  echo
}

check_macos() {
  if [[ "$(uname -s)" != "Darwin" ]]; then
    warn "This installer is for macOS. You appear to be running $(uname -s)."
    warn "Dawnload is a native macOS app (macOS 13 Ventura or newer required)."
    exit 1
  fi
  local major minor
  major="$(sw_vers -productVersion | cut -d. -f1)"
  minor="$(sw_vers -productVersion | cut -d. -f2)"
  if (( major < 13 )); then
    die "Dawnload needs macOS 13 (Ventura) or newer. You are on $major.$minor."
  fi
  ok "macOS $(sw_vers -productVersion)"
}

ensure_command_line_tools() {
  if xcode-select -p >/dev/null 2>&1 && command -v swift >/dev/null 2>&1; then
    ok "Swift toolchain found"
    return
  fi
  notice "The Apple developer command line tools are missing, so the app can be compiled."
  info "A system dialog is about to appear. Click 'Install' and wait for it to finish."
  xcode-select --install >/dev/null 2>&1 || true
  notice "Waiting for the command line tools to finish installing..."
  echo "  (This can take several minutes. Do not close the Apple dialog.)"
  local waited=0
  while ! xcode-select -p >/dev/null 2>&1; do
    sleep 10
    (( waited += 10 ))
    if (( waited > 600 )); then
      die "Timed out waiting for the command line tools. Run 'xcode-select --install' manually, then retry."
    fi
  done
  ok "Command line tools installed"
}

ensure_brew() {
  if command -v brew >/dev/null 2>&1; then
    ok "Homebrew found"
    return
  fi
  notice "Homebrew (the package manager that supplies the download engines) is not installed."
  info "Dawn Labs recommends Homebrew — it is the simplest, safest way to get yt-dlp, gallery-dl, and ffmpeg."
  read -r -p "Install Homebrew now with its official one-liner? [y/N] " confirm
  if [[ "$confirm" != "y" && "$confirm" != "Y" ]]; then
    die "Dawnload needs Homebrew. Install it from https://brew.sh and run this installer again."
  fi
  notice "Installing Homebrew. Enter your macOS password if prompted."
  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
  if [[ -x /opt/homebrew/bin/brew ]]; then
    eval "$(/opt/homebrew/bin/brew shellenv)"
  elif [[ -x /usr/local/bin/brew ]]; then
    eval "$(/usr/local/bin/brew shellenv)"
  fi
  if ! command -v brew >/dev/null 2>&1; then
    die "Homebrew installs finished but it cannot be found. Open a new Terminal and retry."
  fi
  ok "Homebrew installed"
}

ensure_tool() {
  local display="$1" pkg="$2"
  if command -v "$pkg" >/dev/null 2>&1; then
    ok "$display found"
    return
  fi
  notice "$display_name is missing — installing it now."
  brew install "$pkg"
  if ! command -v "$pkg" >/dev/null 2>&1; then
    die "Failed to install $display_name. Please install it with: brew install $pkg"
  fi
  ok "$display_name installed"
}

install_to_applications() {
  notice "Copying Dawnload.app into /Applications"
  if [[ -d "$APP_ATLAS" ]]; then
    info "An older copy exists in /Applications — replacing it."
    rm -rf "$APP_ATLAS"
  fi
  cp -R "$ROOT/.build/${APP_NAME}.app" "$APP_ATLAS"
  ok "Installed to $APP_ATLAS"
  info "First launch: right-click the app and choose 'Open' the first time"
  info "(This is macOS Gatekeeper, not a problem with Dawn — Apple is still getting to know us.)"
}

die() { printf "\033[1;31mError: %s\033[0m\n" "$*" >&2; exit 1; }

main "$@"