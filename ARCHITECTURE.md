# Architecture: `cdd-storage`

`cdd-storage` is the blob persistence layer. 

## Responsibilities
- Receives completed builds (ZIPs, JSON schemas, static documentation bundles) from `cdd-engine` post-generation.
- Stores these artifacts securely mapped by `OrgID`, `RepoID`, and `ReleaseVersion`.
- Provides an extremely fast read path. When a user requests `mydomain.com/u/org/repo`, the gateway forwards the request to `cdd-web-ui` or `cdd-docs-ui`, which in turn requests the raw schema JSON from `cdd-storage` to perform the client-side render or display API documentation.
- Hosts Official CDD Documentation Sites for both Monorepo (`${GITHUB_ORG_NAME}-sdk`) and Polyrepo (`${GITHUB_ORG_NAME}-sdk-${LANGUAGE}`) namespaces via `GET /sites/{org_id}/{repo_id}`, `GET /sites/{org_id}/{repo_id}/{version}`, and `GET /sites/{org_id}/{repo_id}/{version}/{file_path:.*}` with accurate `Content-Type` MIME detection and SHA-256 `ETag` conditional caching.

## Storage Backends
By abstracting the backend via a trait, `cdd-storage` can support:
1. Local File System (Development / On-Prem).
2. AWS S3 / Cloudflare R2 (SaaS / Cloud deployment).

## System Architecture

The following diagram illustrates `cdd-storage`'s role within the broader CDD ecosystem, serving both the generation engine and the frontend clients (like `cdd-web-ui` and `cdd-docs-ui`):

```mermaid
graph TD
    UI[cdd-web-ui]
    Docs[cdd-docs-ui]

    Gateway[cdd-gateway<br/>API Gateway / Ingress]
    API[cdd-control-plane<br/>Backend API / Auth / RBAC]
    Engine[cdd-engine<br/>Core Generator / AST]
    Publisher[cdd-publisher<br/>SDK Publisher Worker]
    Storage[cdd-storage<br/>Blob Storage]
    Registries[(Package Registries<br/>npm, PyPI, crates.io)]

    UI -->|JSON-RPC / REST| Gateway
    Docs -->|Fetch published SDKs/Schemas| Gateway

    Gateway --> API
    Gateway --> Storage

    API --> Engine
    API --> Publisher

    Engine --> Storage
    Publisher -->|Publishes| Registries
```
