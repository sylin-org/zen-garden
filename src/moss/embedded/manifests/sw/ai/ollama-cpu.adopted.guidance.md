---
version: "1"
trigger: post_install
---

# Ollama CPU (Adopted)

**API:** http://{{server-name}}:{{port}}

> This stone is configured for **CPU-only** inference. It uses the same Ollama
> binary as GPU stones but with GPU offloading disabled. Ideal for embedding
> models on thin clients.

{{#if os=windows}}

## Minimal (Admin)

```
setx /M OLLAMA_HOST "0.0.0.0:{{port}}"
setx /M CUDA_VISIBLE_DEVICES ""
setx /M OLLAMA_MAX_LOADED_MODELS "1"
setx /M OLLAMA_NUM_PARALLEL "1"
```

Close Ollama and open it again.

If it still isn't reachable on the LAN, restart the machine.
{{/if}}

{{#if os=linux}}

## One-liner (sudo)

```
sudo sh -c "mkdir -p /etc/systemd/system/ollama.service.d && printf '[Service]\nEnvironment=OLLAMA_HOST=0.0.0.0:{{port}}\nEnvironment=CUDA_VISIBLE_DEVICES=\nEnvironment=OLLAMA_MAX_LOADED_MODELS=1\nEnvironment=OLLAMA_NUM_PARALLEL=1\n' > /etc/systemd/system/ollama.service.d/override.conf" && sudo systemctl daemon-reload && sudo systemctl restart ollama && (command -v ufw >/dev/null && sudo ufw allow {{port}}/tcp || true)
```

{{/if}}

## Recommended Models

For CPU thin clients (2–8 GB RAM), install embedding models:

```
ollama pull all-minilm          # 43 MB  — fast sentence embeddings
ollama pull nomic-embed-text    # 261 MB — higher quality embeddings
```

Avoid generation models larger than 1B parameters on thin clients.
