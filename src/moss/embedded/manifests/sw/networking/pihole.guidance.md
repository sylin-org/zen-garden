---
version: "1"
trigger: post_install
---
# Pi-hole Configuration

Your Pi-hole DNS server is now running on **{{server-name}}**.

## Ports

| Service | Port | Purpose |
|---------|------|---------|
| DNS | {{port}} | DNS queries (TCP/UDP) |
| Web Admin | {{admin-port}} | Admin dashboard |

## Access the Admin Console

Open the web interface at:

```
http://{{server-name}}:{{admin-port}}/admin
```

**Default password:** `pihole`

## Configure Your Network

Set your router's DNS server to `{{server-name}}` (port {{port}}) to block ads for all devices.

Or configure individual devices:

- **Windows:** Settings → Network → DNS → `{{server-name}}`
- **macOS:** System Settings → Network → DNS → `{{server-name}}`
- **Linux:** Edit `/etc/resolv.conf`: `nameserver <IP of {{server-name}}>`
- **Router:** Set DNS server to the IP address of `{{server-name}}`

## Change the Admin Password

For security, change the default password:

```bash
docker exec -it {{name}} pihole -a -p
```

## Update Blocklists

Pi-hole uses blocklists to filter ads. Update them periodically:

```bash
docker exec {{name}} pihole -g
```

## Useful Commands

- **View stats:** `docker exec {{name}} pihole -c`
- **Tail DNS log:** `docker exec {{name}} pihole -t`
- **Whitelist domain:** `docker exec {{name}} pihole -w example.com`
- **Blacklist domain:** `docker exec {{name}} pihole -b ads.example.com`

## Troubleshooting

If ads aren't being blocked:
1. Verify your device is using Pi-hole as DNS (`nslookup pi.hole`)
2. Check Pi-hole is running: `docker exec {{name}} pihole status`
3. Update gravity: `docker exec {{name}} pihole -g`
