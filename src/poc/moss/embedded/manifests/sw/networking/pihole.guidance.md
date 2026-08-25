---
version: "1"
trigger: post_install
---
# Pi-hole

**Admin:** http://{{server-name}}:{{admin-port}}/admin
**Password:** `pihole`

## DNS Configuration

{{#if static-ip}}
**Router (recommended):** Set primary DNS to `{{static-ip}}`

**Per-device:**

**Linux:**
```
echo "nameserver {{static-ip}}" | sudo tee /etc/resolv.conf
```

**macOS:**
```
sudo networksetup -setdnsservers Wi-Fi {{static-ip}}
```

**PowerShell:**
```
Set-DnsClientServerAddress -InterfaceAlias Ethernet -ServerAddresses {{static-ip}}
```

**CMD:**
```
netsh interface ip set dns "Ethernet" static {{static-ip}}
```
{{#else}}
Get this stone's IP, then set it as your router's DNS:

**Bash:**
```
getent hosts {{server-name}} | awk '{print $1}'
```

**PowerShell:**
```
(Resolve-DnsName {{server-name}}).IPAddress
```

**CMD:**
```
nslookup {{server-name}}
```
{{/if}}

## Commands

**Change password:**
```
docker exec -it {{name}} pihole -a -p
```

**Update blocklists:**
```
docker exec {{name}} pihole -g
```

**View stats:**
```
docker exec {{name}} pihole -c
```

**Whitelist domain:**
```
docker exec {{name}} pihole -w example.com
```

{{#if !static-ip}}
**Note:** Consider a DHCP reservation so the IP doesn't change.
{{/if}}
