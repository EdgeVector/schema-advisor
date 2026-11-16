# Schema Advisor

Schema Advisor helps app developers name and type fields and design keys for LastDB. Local review uses the bundled Schema Service vocabulary. LastDB validates the declaration the app adopts.

## Install

Install LastDB and Python 3 on Apple Silicon macOS or Linux x86-64. From the
production stable registry, run:

```sh
lastdb app install schema-advisor --accept-install
schema-advisor --version
```

If your LastDB points to DEV, select production explicitly with `--env prod --channel stable`.
To use the development listing, select `--env dev --channel next`.

The installer verifies the signed listing and the downloaded executable against its pinned SHA-256 digest.

## Review and plan

```sh
schema-advisor review lastdb-app.json
schema-advisor review examples/issue.json --json
schema-advisor plan examples/issue.json --id issue_id --ordered by_project project_id issue_created_at
```

Review recognizes common field aliases and casing variants. Include what a field
identifies or the units it uses so the advisor can check its description before
suggesting a canonical field.

The plan emits a `declaration` to put in the app manifest and `app_supplies_on_write` to map app fields to LastDB key values. In the example, LastDB stores one issue at its ID. The app supplies the project ID and creation time as the named alternate key when it writes the issue. A project lookup returns the same records in creation-time order.

Each ordered query needs a known hash. A native timestamp range can hold several records at one time. Use an explicit ordering convention if the app needs a particular order within a tie.

## Recommend keys from queries

Recommendations and cloud review are available in version 0.2.0 and later.

```sh
schema-advisor recommend examples/issue.json --queries examples/issue-queries.json
```

Describe each query as a lookup or an ordered read within a group. Mark a lookup
with `"identity": true` when the supplied coordinates are stable and uniquely identify
a record. Schema Advisor compares the eligible primary keys and shows the named
alternate keys each choice needs. The output includes the recommended declaration
and the values to supply on writes. In the Issue example, the global issue ID makes
a shorter primary key than the project-and-issue pair. The timeline remains an
ordered alternate key. Starting in 0.2.1, the all-project-issues lookup shares
that key. Use the project ID alone for the full list, or add date bounds for
the timeline. The `query_keys` mapping shows which key each query should use.
Pass `--primary QUERY` to emit the declaration for a different eligible primary key.

## Consult Schema Service

```sh
schema-advisor review examples/issue.json --online --json
```

Use `--online` when you want to send the draft schema definitions to Schema Service.
The command uses production by default. Pass `--env dev` to consult DEV.
If the service cannot answer, review returns local findings from the bundled
vocabulary and reports the fallback. The JSON `source` field tells you where each
schema's findings came from. Review without `--online`, planning and recommendations
all run locally.

## Source

The CLI shares the field advice engine and key validators from LastDB. GitHub Actions builds the CLI and runs the example review, plan and recommendation on Apple Silicon and Linux x86-64.

Each CI build also scores the local executable against synthetic cases for field
matching, draft size and limits, and key planning. The run summary shows the
score and names any mismatches. Download `advice-stress.json` in the platform
artifact for the case details. To rerun the same cases with an installed build,
use `python3 scripts/advice_stress.py --advisor /path/to/schema-advisor`.
