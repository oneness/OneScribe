# Linux GlobalShortcuts Hotkey Debugging Guide

## The Problem

OneScribe tries to use the `org.freedesktop.portal.GlobalShortcuts` D-Bus
portal (via the `ashpd` Rust crate) to register **Alt+Space** as a global
hotkey. On GNOME 50 this bind reliably fails, because the portal backend
crashes.

## Root Cause (confirmed via journal, GNOME 50)

```
OneScribe binary
  └─ ashpd::GlobalShortcuts::create_session()
  └─ ashpd::GlobalShortcuts::bind_shortcuts(["Alt+space"])
  └─ D-Bus → org.freedesktop.portal.Desktop
       └─ xdg-desktop-portal forwards to backend
            └─ xdg-desktop-portal-gnome (the backend)
                 └─ gnome-control-center-global-shortcuts-provider
                      └─ SIGSEGV in cc_global_shortcut_dialog_present
```

`gnome-control-center-global-shortcuts-provider` tries to show a key-binding
dialog to let the user assign the shortcut. Because OneScribe is a
windowless background daemon, there's no parent surface, so
`gdk_surface_get_display` fails its assertion and the process segfaults. The
portal reports this back to the caller as error code 2 ("Other") — confirmed
in the journal:

```bash
journalctl --user | grep -i "global-shortcuts-provider\|core-dump"
```

```
gnome-control-center-global-shortcuts-provider[...]: gdk_surface_get_display: assertion 'GDK_IS_SURFACE (surface)' failed
                    #15 ... cc_global_shortcut_dialog_present (...)
                    #16 ... handle_bind_shortcuts (...)
dbus-...-org.gnome.Settings.GlobalShortcutsProvider@N.service: Main process exited, code=dumped, status=11/SEGV
dbus-...-org.gnome.Settings.GlobalShortcutsProvider@N.service: Failed with result 'core-dump'.
```

This is a GNOME 50 bug, not something OneScribe can work around at the
portal level — there's no way to supply a parent window for a background
daemon.

## The Fix: Automatic Fallback (no manual steps)

The daemon (`hotkey_daemon.rs`) treats the bind failure as non-fatal and
keeps running. On **every startup**, before entering its event loop, it also
registers Alt+Space as a **GNOME custom keyboard shortcut** directly via
`gsettings` (`ensure_gnome_custom_shortcut()`):

- Adds `/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/onescribe/`
  to the `custom-keybindings` list (schema
  `org.gnome.settings-daemon.plugins.media-keys`), preserving any existing
  entries.
- Sets `name` = `OneScribe Toggle`, `binding` = `<Alt>space`, `command` =
  `bash -c 'echo toggle > /run/user/$UID/onescribe-control'` on that
  keybinding's relocatable schema.

The daemon listens on that control FIFO (`/run/user/$UID/onescribe-control`)
independently of the portal session, so pressing Alt+Space toggles recording
through GNOME's own shortcut dispatch instead of the crashing portal path.

This used to require running a separate shell script (`setup-gnome-shortcuts.sh`,
which shelled out to Python to parse/rewrite the gsettings list) once after
install. That script has been removed — the same logic now lives entirely in
Rust and runs automatically every time the daemon starts, so there is no
setup step and no Python dependency.

If a future GNOME release fixes the segfault, `bind_shortcuts` will succeed
and the portal path becomes active with no code change — the custom shortcut
fallback stays configured but simply goes unused.

## Diagnosing a Non-Working Hotkey

### 1. Check the daemon log

```bash
journalctl --user --since "1 hour ago" | grep onescribe
# or, if running manually, check wherever you redirected stdout/stderr
```

Expect to see:

```
GNOME custom shortcut configured: <Alt>space → control FIFO
...
warning: portal shortcut binding failed (Portal request didn't succeed: Other)
  hotkey active via GNOME custom shortcut → control FIFO
```

The portal warning is expected on GNOME 50 and does not indicate a problem.

### 2. Verify the GNOME custom shortcut was registered

```bash
gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings
# should include .../custom-keybindings/onescribe/

gsettings get org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/onescribe/ binding
# '<Alt>space'

gsettings get org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/onescribe/ command
# "bash -c 'echo toggle > /run/user/<uid>/onescribe-control'"
```

If these are missing, `gsettings` may not be available in the daemon's
environment (e.g., a non-GNOME desktop, or a stripped-down session) —
`ensure_gnome_custom_shortcut()` logs a `note:` and no-ops in that case rather
than failing the daemon.

### 3. Verify the control FIFO exists and the daemon is running

```bash
ls -la /run/user/$(id -u)/onescribe-control   # should be a named pipe (prw-...)
pgrep -a onescribe-linux-core
```

### 4. Confirm the crash is the known GNOME 50 bug (optional)

```bash
ps aux | grep gnome-control-center-global-shortcuts
journalctl --user | grep -i "core-dump"
```

If you see `Failed with result 'core-dump'` for
`org.gnome.Settings.GlobalShortcutsProvider`, this is the known upstream bug
described above — no action needed on OneScribe's part.

## Why the Portal Path Is Still Attempted

The daemon still calls `bind_shortcuts` on every session rather than skipping
it outright, so that:

- The daemon works unmodified on GNOME versions/desktops where the portal
  bind succeeds (e.g., some GNOME 49.x builds where the backend grabs the key
  directly without the dialog).
- If GNOME fixes the GNOME 50 segfault upstream, OneScribe picks up the fix
  automatically with no release needed on our side.
