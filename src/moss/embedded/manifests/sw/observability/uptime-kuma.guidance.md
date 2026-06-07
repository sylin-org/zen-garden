---
version: "1"
trigger: post_install
---
# Uptime Kuma

Uptime Kuma is running on **{{server-name}}** — watch your services and get alerts
when they go down.

## Open it

```
http://{{server-name}}:{{port}}
```

On first visit you'll create your **admin account** (there is no default password),
then add monitors — HTTP(s), TCP, ping, DNS, Docker, and more — and wire up
notification channels (Telegram, Discord, ntfy, email, ~90 others).

## Status pages

Publish a public status page for any group of monitors — handy for sharing uptime
with users.

Everything (the database, your settings, monitor history) lives on the
`uptime-kuma-data` volume and is captured by snapshots.
