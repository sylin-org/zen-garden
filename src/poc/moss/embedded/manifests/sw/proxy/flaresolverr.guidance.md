---
version: "1"
trigger: post_install
---
# FlareSolverr

FlareSolverr is running on **{{server-name}}**. It is a proxy that other services
call to solve Cloudflare/DDoS-Guard challenges — point your indexer manager at it.

## Prowlarr / Jackett

Add an indexer proxy of type **FlareSolverr** with this URL:

```
http://{{server-name}}:{{port}}
```

## Test it

```
curl -L -X POST "http://{{server-name}}:{{port}}/v1" -H "Content-Type: application/json" -d '{"cmd":"request.get","url":"https://www.google.com","maxTimeout":60000}'
```

Sessions are in-memory only — a restart clears all cookies. The bundled captcha
solvers are non-functional upstream, so leave `CAPTCHA_SOLVER` at `none`.
