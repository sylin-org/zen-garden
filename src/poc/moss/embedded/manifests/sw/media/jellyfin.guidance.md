---
version: "1"
trigger: post_install
---
# Jellyfin

Jellyfin is running on **{{server-name}}** — your media server.

## Open it

```
http://{{server-name}}:{{port}}
```

The first time you open it, a setup wizard creates your admin account and adds
libraries. Point the libraries at the media folders you mount onto this stone.

## Hardware transcoding (optional)

This offering uses **software (CPU) transcoding** by default, which works on any
stone. Hardware acceleration (Intel/AMD `/dev/dri` or NVIDIA) needs device
passthrough the offering does not yet configure — CPU transcoding or direct-play
is the out-of-the-box path.
