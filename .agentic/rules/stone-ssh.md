---
globs: installer/**/*.ps1, scripts/**/*.ps1
alwaysApply: false
---
# Stone SSH Operations

## Credentials
- **User**: `stone`
- **Password**: `stone`

## SSH Commands (PowerShell)

### First-time connection (accept host key)
```powershell
echo y | plink -ssh "stone@<stone-name>" -pw stone "echo OK"
```

### Run command on stone
```powershell
plink -batch -ssh "stone@<stone-name>" -pw stone "<command>"
```

### Examples
```powershell
# Check moss status
plink -batch -ssh "stone@stone-crystal-forest" -pw stone "systemctl status garden-moss"

# View logs
plink -batch -ssh "stone@stone-crystal-forest" -pw stone "sudo journalctl -u garden-moss -n 50"

# Follow logs
plink -batch -ssh "stone@stone-crystal-forest" -pw stone "sudo journalctl -u garden-moss -f"

# Restart moss
plink -batch -ssh "stone@stone-crystal-forest" -pw stone "sudo systemctl restart garden-moss"

# Docker status
plink -batch -ssh "stone@stone-crystal-forest" -pw stone "docker ps"

# Container logs
plink -batch -ssh "stone@stone-crystal-forest" -pw stone "docker logs zen-offering-mongodb"

# Firmware updates
plink -batch -ssh "stone@stone-crystal-forest" -pw stone "fwupdmgr get-updates"
```

## Common Remote Commands

| Purpose | Command |
|---------|---------|
| Moss status | `systemctl status garden-moss` |
| Moss logs (last 50) | `sudo journalctl -u garden-moss -n 50` |
| Moss logs (follow) | `sudo journalctl -u garden-moss -f` |
| Restart Moss | `sudo systemctl restart garden-moss` |
| Docker containers | `docker ps` |
| Container logs | `docker logs <container>` |
| Firmware check | `fwupdmgr get-updates` |
| Firmware history | `fwupdmgr get-history` |
| Disk usage | `df -h` |
| Memory | `free -h` |

## Batch Operations

When operating on multiple stones:
```powershell
$stones = @("stone-crystal-forest", "stone-mossy-brook", "stone-quiet-pond")
foreach ($stone in $stones) {
    Write-Host "=== $stone ===" -ForegroundColor Cyan
    plink -batch -ssh "stone@$stone" -pw stone "systemctl status garden-moss"
}
```

## File Transfer (SCP)

```powershell
# Upload to stone
pscp -pw stone "local-file.txt" "stone@<stone-name>:/home/stone/"

# Download from stone
pscp -pw stone "stone@<stone-name>:/var/lib/zen-garden/file.json" "."
```
