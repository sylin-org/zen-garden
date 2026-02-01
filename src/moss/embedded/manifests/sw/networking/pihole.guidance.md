---
version: "1"
trigger: post_install
---
# Pi-hole Configuration

Your Pi-hole DNS server is now running on **{{server-name}}**.

## Service Ports

- **DNS:** port {{port}} (TCP/UDP queries)
- **Web Admin:** port {{admin-port}} (dashboard)

## Access the Admin Console

```
http://{{server-name}}:{{admin-port}}/admin
```

**Default password:** `pihole`

## How Pi-hole Works

Pi-hole acts as a DNS filter between your devices and the internet:

1. Your device asks Pi-hole to resolve a domain
2. Pi-hole checks if it's on a blocklist
3. If blocked → returns 0.0.0.0 (ads don't load)
4. If allowed → forwards to upstream DNS (Google, Cloudflare, your router, etc.)

Configure upstream DNS servers in the Pi-hole admin panel under **Settings → DNS**.

## Important: Prevent IP Changes

If the Pi-hole stone's IP changes (DHCP lease renewal), devices will lose DNS resolution. Prevent this with one of these approaches:

**Option 1: DHCP Reservation (Recommended)**
In your router's DHCP settings, reserve a static IP for `{{server-name}}`'s MAC address. This ensures the same IP after reboots/renewals.

**Option 2: Let Pi-hole Serve DHCP**
Pi-hole can act as your DHCP server (Admin → Settings → DHCP). It automatically configures all clients to use itself as DNS. Disable DHCP on your router first.

**Option 3: Static IP on the Stone**
Configure the stone's network interface with a static IP outside your router's DHCP range.

## Configure Your Network

**Option A: Router-level (recommended)**
Set your router's DNS server to the Pi-hole IP. All devices get ad-blocking automatically.

**Option B: Per-device**
Configure individual devices to use Pi-hole as their DNS server.

### Get Your Pi-hole IP

If your system supports mDNS, use `{{server-name}}` directly. Otherwise, get the IP:

```bash
ping {{server-name}}
```

### Linux (systemd-resolved)

```bash
# Temporary (until reboot)
sudo resolvectl dns eth0 $(getent hosts {{server-name}} | awk '{print $1}')

# Permanent: edit /etc/systemd/resolved.conf
# DNS=<pi-hole-ip>
```

### Linux (resolv.conf)

```bash
# Get the IP first
PIHOLE_IP=$(getent hosts {{server-name}} | awk '{print $1}')
echo "nameserver $PIHOLE_IP" | sudo tee /etc/resolv.conf
```

### Windows PowerShell (Run as Admin)

```powershell
# Find your adapter name first
Get-NetAdapter | Select-Object Name, Status

# Set DNS (replace "Ethernet" with your adapter name)
$ip = [System.Net.Dns]::GetHostAddresses("{{server-name}}")[0].IPAddressToString
Set-DnsClientServerAddress -InterfaceAlias "Ethernet" -ServerAddresses $ip
```

### Windows CMD (Run as Admin)

```cmd
:: Get IP first
ping {{server-name}}
:: Then set DNS (replace IP and "Ethernet" with your values)
netsh interface ip set dns "Ethernet" static <pi-hole-ip>
```

### macOS

```bash
# Get IP and set DNS for Wi-Fi
PIHOLE_IP=$(dig +short {{server-name}} @224.0.0.251 -p 5353 | head -1)
sudo networksetup -setdnsservers "Wi-Fi" $PIHOLE_IP
```

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

1. Verify your device is using Pi-hole as DNS: `nslookup pi.hole`
2. Check Pi-hole is running: `docker exec {{name}} pihole status`
3. Update gravity: `docker exec {{name}} pihole -g`

**Tip:** Visit `pi.hole/admin` from a configured device - if it loads, DNS is working.
