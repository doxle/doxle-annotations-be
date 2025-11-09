# Doxle Core

ALIAS=prod ./scripts/deploy_lambda.sh

Shared libraries for the Doxle platform.

## Crates

- **core** - Core types and utilities
- **database** - Database models and operations
- **auth** - Authentication and authorization
- **ui-components** - Shared UI components

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
doxle-core = { git = "https://github.com/doxle/doxle-core", branch = "main" }
doxle-database = { git = "https://github.com/doxle/doxle-core", branch = "main" }
doxle-auth = { git = "https://github.com/doxle/doxle-core", branch = "main" }
doxle-ui-components = { git = "https://github.com/doxle/doxle-core", branch = "main" }
```
