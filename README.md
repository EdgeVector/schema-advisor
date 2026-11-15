# Schema Advisor

Schema Advisor helps app developers name and type fields and plan keys for LastDB. It works offline with the built-in Schema Service vocabulary. Review findings are suggestions. LastDB validates the declaration the app adopts.

## Install

Install LastDB and Python 3 on Apple Silicon macOS or Linux x86-64. For the
development listing, use:

```sh
lastdb app install schema-advisor --env dev --channel next --accept-install
schema-advisor --version
```

For an app version listed in your default registry channel, `lastdb app install
schema-advisor --accept-install` uses that channel.

The registry verifies the app listing. The installer verifies the downloaded executable against the SHA-256 digest pinned in this source. Advice and planning work locally after installation.

## Review and plan

```sh
schema-advisor review lastdb-app.json
schema-advisor review examples/issue.json --json
schema-advisor plan examples/issue.json --id issue_id --ordered by_project project_id issue_created_at
```

The plan emits a `declaration` to put in the app manifest and `app_supplies_on_write` to map app fields to LastDB key values. In the example, LastDB stores one issue at its ID. The app supplies the project ID and creation time as the named alternate key when it writes the issue. A project lookup returns the same records in creation-time order.

Start with the queries the app needs. Each ordered query needs a known hash. A native timestamp range can hold several records at one time. Use an explicit ordering convention if the app needs a particular order within a tie.

## Source

The CLI shares the field advice engine and key validators from LastDB. GitHub Actions builds the CLI and runs the example review and plan on Apple Silicon and Linux x86-64.
