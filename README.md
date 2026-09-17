# liment

Simple tray application that shows LLM usage limits.

## Configuration

The config file is located at `~/.config/liment/config.toml` and is created automatically on first launch.

### Claude Code

Uses the Claude Code OAuth token from the system keychain. No extra configuration needed.
When the access token expires, liment refreshes it itself and writes the new tokens back to the keychain,
so Claude Code and liment stay logged in together.

```toml
provider = "claude_code"
```

Optionally, you can provide a token manually:

```toml
provider = "claude_code"

[settings.claude_code]
token = "sk-ant-..."
```

### Codex

Uses the ChatGPT token from `~/.codex/auth.json` (written by `codex login`; `CODEX_HOME` is honored). No extra configuration needed.
When the access token expires, liment refreshes it itself and writes the new tokens back to `auth.json`,
so the Codex CLI and liment stay logged in together.

```toml
provider = "codex"
```

Optionally, you can provide the token and account id manually:

```toml
provider = "codex"

[settings.codex]
token = "eyJ..."
account_id = "e0204854-c988-4e72-9edc-6bb0d1c35e9b"
```

### CLIProxy Claude

Proxies requests through a [CLIProxy](https://github.com/nicholasgasior/cliproxy) instance.

```toml
provider = "cliproxy_claude"

[settings.cliproxy_claude]
base_url = "http://localhost:8317"
management_token = "your-management-secret"
auth_index = "0"
```

### CLIProxy Codex

Fetches auth metadata via CLIProxy management API, then proxies `wham/usage`.

```toml
provider = "cliproxy_codex"

[settings.cliproxy_codex]
base_url = "http://localhost:8317"
management_token = "your-management-secret"
auth_index = "1b3ba41df68b1b45"
```

### Notifications

All control center notifications on macOS require the app to be code-signed. If you're running a non-signed build (which you probably are, since release builds are not signed), you can either:

- `/Applications/liment.app/Contents/MacOS/liment --self-sign` to ad-hoc sign and relaunch.
- Enable automatic signing in the config, which effectively does the same thing but on startup.

```toml
auto_codesign = true
```

This will ad-hoc sign the app on startup if needed and relaunch it automatically.

### General options

```toml
# Whether to render the tray icon in monochrome.
monochrome_icon = true

# Display mode: "usage" or "remaining".
display_mode = "usage"

# Which usage windows to show in the tray, by short label, in order (e.g. ["5h"] or ["7d", "5h"]).
# Empty = the first two available. At most two are used, unknown labels are skipped.
# A single window is drawn stacked (label over percentage) instead of in one line.
tray_windows = []

# Whether to show period percentage next to "resets in".
show_period_percentage = false

# Reset time format: "relative" (resets in 3h) or "absolute" (resets on 13 Feb, 14:00).
reset_time_format = "relative"

# How often to refetch usage data, in seconds.
refetch_interval = 450
```

## License

Distributed under the The Unlicense, except for the Claude logo.
