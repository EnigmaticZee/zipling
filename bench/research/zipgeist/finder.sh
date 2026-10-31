#!/bin/zsh
# finder.sh <folder>: one Finder window on <folder>, list view, same place every time.
osascript -e 'tell application "Finder" to close every window' \
  -e "tell application \"Finder\" to open (POSIX file \"$1\" as alias)" \
  -e 'tell application "Finder" to set bounds of front window to {600, 140, 1400, 640}' \
  -e 'tell application "Finder" to set current view of front window to list view' \
  -e 'tell application "Finder" to activate'
