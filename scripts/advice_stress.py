#!/usr/bin/env python3
"""Score Schema Advisor offline against synthetic field and key cases."""

import argparse
import json
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory


USER_DESCRIPTION = "a stable opaque identifier for a user"


def exact(kind):
    return kind, None, None


def evidence(kind, key, value):
    return kind, key, value


def near(name):
    return evidence("near_match", "suggested_name", name)


def field_case(case_id, field, field_type="String", *, description=None,
               version=None, classification=None, want=(), avoid=()):
    schema = {"name": case_id, "fields": [field]}
    if field_type is not None:
        schema["field_types"] = {field: field_type}
    if description is not None:
        schema["field_descriptions"] = {field: description}
    if version is not None:
        schema["field_versions"] = {field: version}
    if classification is not None:
        schema["field_data_classifications"] = {field: classification}
    return case_id, schema, want, avoid


FIELD_CASES = [
    field_case("exact_user_id", "user_id", description=USER_DESCRIPTION,
               want=[exact("canonical_match")]),
    field_case("case_and_punctuation", "user_id",
               description="A STABLE, opaque identifier for a user!",
               want=[exact("canonical_match")]),
    field_case("equivalent_paraphrase", "user_id",
               description="an opaque and stable identifier assigned to a user",
               want=[exact("canonical_match")], avoid=[exact("same_name_different_meaning")]),
    field_case("different_concept_same_name", "user_id",
               description="the unique identifier for a scheduled delivery",
               want=[exact("same_name_different_meaning")]),
    field_case("different_type_same_name", "user_id", "Integer", description=USER_DESCRIPTION,
               want=[evidence("same_name_different_meaning", "suggested_type", "String")]),
    field_case("different_version_same_name", "user_id", version=2,
               want=[exact("same_name_different_meaning")]),
    field_case("missing_canonical_type", "user_id", None,
               want=[evidence("missing_type", "suggested_type", "String")]),
    field_case("provided_classification", "user_id", description=USER_DESCRIPTION,
               classification={"data_domain": "identity", "sensitivity_level": 3},
               want=[exact("canonical_match")], avoid=[exact("suggested_classification")]),
    field_case("canonical_vendor_id", "vendor_id",
               want=[exact("canonical_match")], avoid=[exact("near_match")]),
    field_case("prefix_overlap", "owner_user_id", description=USER_DESCRIPTION,
               want=[near("user_id")]),
    field_case("same_words_reordered", "id_user", description=USER_DESCRIPTION,
               want=[near("user_id")]),
    field_case("repeated_tokens", "user_user_user_id", description=USER_DESCRIPTION,
               want=[near("user_id")]),
    field_case("hyphenated_uppercase", "USER-ID", description=USER_DESCRIPTION,
               want=[exact("abbreviation"), near("user_id")]),
    field_case("non_ascii_spelling", "usér_id", description=USER_DESCRIPTION,
               want=[exact("abbreviation")]),
    field_case("camel_case", "userId",
               want=[exact("abbreviation"), near("user_id")]),
    field_case("alias_with_words_removed", "contact_email",
               description="an email address belonging to a person or entity",
               want=[near("contact_email_address")]),
    field_case("common_purchase_order_alias", "purchase_order_id",
               description="a unique identifier for a purchase order",
               want=[near("order_id")]),
    field_case("width_implies_pixels", "image_width", "Integer",
               description="the pixel width of an image", want=[near("image_width_pixels")]),
    field_case("candidate_type_filter", "image_width", "String",
               description="a human-entered width label", avoid=[near("image_width_pixels")]),
    field_case("generic_single_word", "number", want=[exact("abbreviation")]),
    field_case("unrelated_unknown_type", "retry_backoff_ceiling", None,
               want=[exact("missing_type")], avoid=[exact("near_match")]),
    field_case("milliseconds_guard", "video_duration_milliseconds", "Number",
               description="the duration of a video in milliseconds",
               avoid=[near("video_duration_seconds")]),
    field_case("bits_guard", "file_size_bits", "Integer",
               description="the size of a file in bits", avoid=[near("file_size_bytes")]),
    field_case("target_source_guard", "content_target_url",
               description="the destination URL for content",
               avoid=[near("content_source_url")]),
    field_case("radians_vs_degrees", "geographic_latitude_radians", "Number",
               description="geographic latitude in radians",
               avoid=[near("geographic_latitude_degrees")]),
    field_case("send_vs_delete", "message_deleted_at", "DateTime",
               description="when a message was deleted", avoid=[near("message_sent_at")]),
    field_case("cooking_vs_cooling", "recipe_cooling_minutes", "Number",
               description="time spent letting the finished food cool",
               avoid=[near("recipe_cooking_minutes"), near("recipe_preparation_minutes")]),
    field_case("message_vs_delivery_id", "message_delivery_id", None,
               description="the identifier of one delivery attempt, with multiple attempts per message",
               want=[exact("missing_type")], avoid=[near("message_id")]),
    field_case("order_vs_customer_id", "order_customer_id",
               description="identifier of the customer who placed an order",
               avoid=[near("order_id")]),
    field_case("semantic_mime_alias", "mime_type",
               description="the MIME type of a file", want=[near("file_media_type")]),
    field_case("semantic_surname_alias", "person_surname",
               description="a person's family or surname", want=[near("person_family_name")]),
    field_case("semantic_birthdate_alias", "person_birthdate", "Date",
               description="a person's date of birth", want=[near("date_of_birth")]),
    field_case("transaction_typo", "transactionn_id",
               description="a unique identifier for a financial transaction",
               want=[near("transaction_id")]),
    field_case("time_update_vs_create", "photo_created_at", "DateTime",
               description="when a photo file was created by an import",
               avoid=[near("photo_taken_at")]),
    field_case("ambiguous_name_abstains", "person_name",
               description="the name shown on this person's profile",
               avoid=[exact("near_match")]),
    field_case("curated_alias_without_description", "mime_type", None,
               want=[near("file_media_type"), evidence("missing_type", "suggested_type", "String")]),
    field_case("alias_with_conflicting_description", "mime_type",
               description="the MIME type of a message",
               avoid=[near("file_media_type")]),
    field_case("units_conflict_with_identical_description", "geographic_latitude_radians", "Number",
               description="the geographic latitude north of the equator, in decimal degrees",
               avoid=[near("geographic_latitude_degrees")]),
    field_case("different_meaning_with_missing_type", "user_id", None,
               description="the unique identifier for a scheduled delivery",
               want=[exact("same_name_different_meaning"), exact("missing_type")],
               avoid=[evidence("missing_type", "suggested_type", "String"),
                      exact("suggested_classification")]),
]


def run(advisor, args, payload=None):
    return subprocess.run(
        [advisor, *args],
        input=None if payload is None else json.dumps(payload),
        capture_output=True,
        text=True,
        timeout=20,
        check=False,
    )


def require_json(reply):
    if reply.returncode:
        command = " ".join(reply.args)
        raise RuntimeError(f"{command} exited {reply.returncode}: {reply.stderr.strip()}")
    return json.loads(reply.stdout)


def matches(findings, target):
    kind, key, value = target
    return any(
        item["kind"] == kind and (key is None or item["evidence"].get(key) == value)
        for item in findings
    )


def assess_fields(advisor):
    manifest = {"app_id": "advice-stress", "schemas": [case[1] for case in FIELD_CASES]}
    reports = require_json(run(advisor, ["review", "-", "--json"], manifest))
    by_name = {report["schema"]: report["findings"] for report in reports}
    if len(reports) != len(FIELD_CASES) or len(by_name) != len(FIELD_CASES):
        raise RuntimeError("Field review returned missing or repeated case results.")
    rows = []
    for case_id, schema, want, avoid in FIELD_CASES:
        findings = by_name[case_id]
        missing = [target for target in want if not matches(findings, target)]
        unwanted = [target for target in avoid if matches(findings, target)]
        rows.append({
            "case": case_id,
            "draft": schema,
            "wanted": want,
            "avoided": avoid,
            "met_expectation": not missing and not unwanted,
            "missing": missing,
            "unwanted": unwanted,
            "findings": findings,
        })
    return rows


def assess_sizes(advisor):
    rows = []
    for size in (128, 512, 513):
        fields = [f"draft_user_reference_probe{index:03d}" for index in range(size)]
        described = set(fields[::2])
        draft = {
            "name": "DenseReview",
            "fields": fields,
            "field_types": dict.fromkeys(fields, "String"),
            "field_descriptions": dict.fromkeys(sorted(described), USER_DESCRIPTION),
        }
        response = run(advisor, ["review", "-", "--json"], draft)
        if size == 513:
            message = response.stderr.strip()
            rows.append({
                "case": "513_field_limit",
                "field_count": size,
                "met_expectation": response.returncode != 0 and "at most 512" in message,
                "exit_code": response.returncode,
                "error": message,
            })
            continue

        report = require_json(response)[0]
        findings = report["findings"]
        expected = [(field, "near_match", "user_id") for field in fields if field in described]
        actual = [
            (item["field"], item["kind"], item["evidence"].get("suggested_name"))
            for item in findings
        ]
        rows.append({
            "case": f"{size}_field_review",
            "field_count": size,
            "met_expectation": report["schema"] == draft["name"] and expected == actual,
            "expected_findings": len(expected),
            "actual_findings": len(actual),
            "missing_findings": sorted(set(expected) - set(actual)),
            "unexpected_findings": sorted(set(actual) - set(expected)),
        })
    return rows


DRAFT = {
    "name": "Issue",
    "fields": ["issue_id", "project_id", "reporter_id", "issue_created_at", "issue_updated_at"],
    "field_types": {
        "issue_id": "String", "project_id": "String", "reporter_id": "String",
        "issue_created_at": "DateTime", "issue_updated_at": "DateTime",
    },
}


def query(name, hash_field, range_field=None, *, kind="lookup", identity=False):
    result = {"name": name, "hash": hash_field, "kind": kind}
    if range_field:
        result["range"] = range_field
    if identity:
        result["identity"] = True
    return result


BY_ISSUE = query("by_issue", "issue_id", identity=True)
PROJECT_LIST = query("project_list", "project_id")
PROJECT_CREATED = query("project_created", "project_id", "issue_created_at", kind="ordered")
PROJECT_UPDATED = query("project_updated", "project_id", "issue_updated_at", kind="ordered")
PROJECT_ISSUE = query("project_issue", "project_id", "issue_id", identity=True)
REPORTER_LIST = query("reporter_list", "reporter_id")
QUERY_CASES = [
    ("share_project_key", [BY_ISSUE, PROJECT_LIST, PROJECT_CREATED], None,
     {"by_issue": "primary", "project_list": "project_created", "project_created": "project_created"}),
    ("keep_distinct_range_order", [BY_ISSUE, PROJECT_LIST, PROJECT_CREATED, PROJECT_UPDATED], None,
     {"by_issue": "primary", "project_list": "project_created", "project_created": "project_created",
      "project_updated": "project_updated"}),
    ("keep_distinct_hash", [BY_ISSUE, PROJECT_LIST, PROJECT_CREATED, REPORTER_LIST], None,
     {"by_issue": "primary", "project_list": "project_created", "project_created": "project_created",
      "reporter_list": "reporter_list"}),
    ("share_hash_range_primary", [BY_ISSUE, PROJECT_LIST, PROJECT_CREATED, PROJECT_ISSUE],
     "project_issue", {"by_issue": "by_issue", "project_list": "primary",
                       "project_created": "project_created", "project_issue": "primary"}),
    ("keep_hash_only_identity", [BY_ISSUE, PROJECT_LIST, PROJECT_CREATED,
                                 query("singleton_project", "project_id", identity=True)],
     None, {"by_issue": "primary", "project_list": "project_list",
            "project_created": "project_created", "singleton_project": "project_list"}),
    ("no_identity", [PROJECT_LIST, PROJECT_CREATED], None, "mark a query identity: true"),
    ("bad_hash_type", [BY_ISSUE, query("bad_date_hash", "issue_created_at")], None,
     "declared String hash field"),
    ("128_queries_share_one_alternate",
     [BY_ISSUE, PROJECT_CREATED, *[query(f"project_view_{i:03d}", "project_id") for i in range(126)]],
     None, {"by_issue": "primary", "project_created": "project_created",
            **{f"project_view_{i:03d}": "project_created" for i in range(126)}}),
]


def assess_keys(advisor):
    rows = []
    with TemporaryDirectory(prefix="schema-advisor-inputs-") as temp:
        queries_path = Path(temp) / "queries.json"
        for case_id, queries, primary, expected in QUERY_CASES:
            queries_path.write_text(json.dumps(queries), encoding="utf-8")
            args = ["recommend", "-", "--queries", str(queries_path)]
            if primary:
                args.extend(["--primary", primary])
            response = run(advisor, args, DRAFT)
            if isinstance(expected, str):
                rows.append({"case": case_id, "queries": queries, "expected_error": expected,
                             "met_expectation": response.returncode != 0 and expected in response.stderr,
                             "error": response.stderr.strip()})
                continue
            output = require_json(response)
            selected = output["recommended"]
            actual_alternates = set(output["declaration"]["record_keys"].get("alternates", {}))
            expected_alternates = set(expected.values()) - {"primary"}
            rows.append({
                "case": case_id,
                "queries": queries,
                "requested_primary": primary,
                "expected_query_keys": expected,
                "actual_query_keys": selected["query_keys"],
                "actual_alternates": sorted(actual_alternates),
                "met_expectation": (selected["query_keys"] == expected
                                    and actual_alternates == expected_alternates),
            })
    return rows



def print_summary(report):
    print(f"### Offline advice stress checks ({report['advisor']})")
    print()
    if "error" in report:
        print(f"Could not score the run. {report['error']}")
        return

    print("| Area | Passed | Cases |")
    print("| --- | ---: | ---: |")
    for name, result in report["summary"].items():
        print(f"| {name} | {result['passed']} | {result['cases']} |")
    print()
    print(f"**{report['passed']}/{report['cases']} expectations met.**")
    failures = [
        f"{area} / {row['case']}"
        for area, cases in report["results"].items()
        for row in cases
        if not row["met_expectation"]
    ]
    if failures:
        print()
        print("Cases with unexpected results")
        print()
        for failure in failures:
            print(f"- `{failure}`")


def score(advisor):
    version = run(advisor, ["--version"])
    version.check_returncode()
    results = {
        "Field review": assess_fields(advisor),
        "Draft size and limits": assess_sizes(advisor),
        "Key planning": assess_keys(advisor),
    }
    summary = {
        name: {"passed": sum(row["met_expectation"] for row in rows), "cases": len(rows)}
        for name, rows in results.items()
    }
    return {
        "advisor": version.stdout.strip(),
        "passed": sum(row["passed"] for row in summary.values()),
        "cases": sum(row["cases"] for row in summary.values()),
        "summary": summary,
        "results": results,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--advisor", default="schema-advisor", help="path to the executable")
    parser.add_argument("--report", type=Path, help="write the full JSON case results to this file")
    args = parser.parse_args()
    try:
        report = score(args.advisor)
    except (OSError, subprocess.SubprocessError, ValueError, KeyError, TypeError, RuntimeError) as error:
        report = {"advisor": args.advisor, "error": str(error)}
    print_summary(report)
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return int("error" in report or report["passed"] != report["cases"])


if __name__ == "__main__":
    raise SystemExit(main())
