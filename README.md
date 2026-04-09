# liment

Simple tray application that shows LLM usage limits.

## Configuration

The config file is located at `~/.config/liment/config.toml` and is created automatically on first launch.

### Claude Code

Uses the Claude Code OAuth token from the system keychain. No extra configuration needed.

```toml
provider = "claude_code"
```

Optionally, you can provide a token manually:

```toml
provider = "claude_code"

[settings.claude_code]
token = "sk-ant-..."
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

# Whether to show period percentage next to "resets in".
show_period_percentage = false

# Reset time format: "relative" (resets in 3h) or "absolute" (resets on 13 Feb, 14:00).
reset_time_format = "relative"

# How often to refetch usage data, in seconds.
refetch_interval = 450
```

## License

Distributed under the The Unlicense, except for the Claude logo.
