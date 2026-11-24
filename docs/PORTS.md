# Port Allocation & Network Exposure — ABIFR

Verified against active Windows host TCP listeners (`1001, 5000, 5040, 7680, 8000, 9090, 22346, 24830, 27015, 42050, 49664..49676`). Default dev ports (`3000`, `5432`, `6379`, `8000`, `8080`) are intentionally avoided to guarantee full project isolation.

| Service | Host | Port | Config Variable | Exposure |
|---|---|---|---|---|
| Next.js Frontend | `127.0.0.1` | `14300` | `FRONTEND_PORT` | Local UI |
| Rust Backend API (Axum) | `127.0.0.1` | `14800` | `BACKEND_PORT` | Local API |
| Python Document Intelligence | `127.0.0.1` | `14900` | `DOC_INTEL_PORT` | Internal only (`X-Internal-Token`) |
| Managed PostgreSQL 16 | `127.0.0.1` | `14543` | `POSTGRES_PORT` | Internal only |
| Redis (Optional) | `127.0.0.1` | `14637` | `REDIS_PORT` | Internal only (optional) |

All ports and bind hosts can be overridden via `.env` or shell environment variables before running `scripts/launch.ps1`.
