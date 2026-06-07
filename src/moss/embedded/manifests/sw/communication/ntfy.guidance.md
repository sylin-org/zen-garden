---
version: "1"
trigger: post_install
---
# ntfy

ntfy is running on **{{server-name}}** — send push notifications to your phone or
desktop over plain HTTP.

## Send a test notification

Subscribe to a topic in the ntfy mobile/desktop app (point it at
`http://{{server-name}}:{{port}}`), then publish from anywhere:

```
curl -d "Hello from Zen Garden" http://{{server-name}}:{{port}}/mytopic
```

Any script, cron job, or service can post to a topic the same way — no client
library required.

## It's open by default

Anyone who can reach this server can read and write any topic. To lock it down,
set `NTFY_AUTH_DEFAULT_ACCESS=deny-all` and create users with `ntfy user add`.

## For attachments and mobile delivery

Set `NTFY_BASE_URL=http://{{server-name}}:{{port}}` (or your public URL) so
attachment links and click actions resolve correctly.

The message cache, attachments, and user database live on the `ntfy-cache` and
`ntfy-data` volumes and are captured by snapshots.
