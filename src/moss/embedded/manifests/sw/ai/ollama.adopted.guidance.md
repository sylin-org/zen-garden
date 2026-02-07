---
version: "1"
trigger: post_install
---

# Ollama (Adopted)

**API:** http://{{server-name}}:{{port}}

{{#if os=windows}}

## Minimal (Admin)

```
setx /M OLLAMA_HOST "0.0.0.0:{{port}}"
```

Close Ollama and open it again.

If it still isn't reachable on the LAN, restart the machine.
{{/if}}

{{#if os=linux}}

## One-liner (sudo)

```
sudo sh -c "mkdir -p /etc/systemd/system/ollama.service.d && printf '[Service]\nEnvironment=OLLAMA_HOST=0.0.0.0:{{port}}\n' > /etc/systemd/system/ollama.service.d/override.conf" && sudo systemctl daemon-reload && sudo systemctl restart ollama && (command -v ufw >/dev/null && sudo ufw allow {{port}}/tcp || true)
```

{{/if}}

{{#if os=macos}}

## One-liner (Admin)

```
sudo sh -c "launchctl setenv OLLAMA_HOST 0.0.0.0:{{port}}" && sudo pkill -f ollama && nohup ollama serve >/tmp/ollama.log 2>&1 &
```

{{/if}}
