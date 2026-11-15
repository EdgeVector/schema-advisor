use crate::plan::Draft;
use schema_types::{
    record_keys::{RecordKeyDefinition, RecordKeyKind, RecordKeys},
    DeclarativeSchemaDefinition, FieldValueType,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Coordinates<'a> {
    hash: &'a str,
    range: Option<&'a str>,
}

impl Coordinates<'_> {
    fn mapping(&self) -> Value {
        match self.range {
            Some(range) => json!({ "hash_from": self.hash, "range_from": range }),
            None => json!({ "hash_from": self.hash }),
        }
    }
}

struct Query<'a> {
    name: &'a str,
    ordered: bool,
    identity: bool,
    coordinates: Coordinates<'a>,
    key: RecordKeyDefinition,
}

impl<'a> Query<'a> {
    fn parse(value: &'a Value, draft: &Draft, index: usize) -> Result<Self, String> {
        let query = value
            .as_object()
            .ok_or_else(|| format!("query {} must be an object", index + 1))?;
        let name = query
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("query {} needs a name", index + 1))?;
        DeclarativeSchemaDefinition::validate_field_name(name)?;
        if name == "primary" {
            return Err("primary is reserved as a query key name".into());
        }
        if let Some(field) = query.keys().find(|key| {
            !matches!(
                key.as_str(),
                "name" | "kind" | "hash" | "range" | "identity"
            )
        }) {
            return Err(format!("{name}: unknown query property {field}"));
        }

        let ordered = match query.get("kind").and_then(Value::as_str) {
            Some("lookup") => false,
            Some("ordered") => true,
            _ => return Err(format!("{name}: kind must be lookup or ordered")),
        };
        let identity = match query.get("identity") {
            Some(value) => value
                .as_bool()
                .ok_or_else(|| format!("{name}: identity must be a boolean"))?,
            None => false,
        };
        let hash = query
            .get("hash")
            .and_then(Value::as_str)
            .filter(|field| draft.field_type(field) == Some(&FieldValueType::String))
            .ok_or_else(|| format!("{name}: supply a known, declared String hash field"))?;
        let (range, range_type) = match query.get("range") {
            Some(value) => {
                let range = value
                    .as_str()
                    .ok_or_else(|| format!("{name}: range must name one declared field"))?;
                let range_type = draft
                    .field_type(range)
                    .ok_or_else(|| format!("{name}: {range:?} is not a declared range field"))?;
                (Some(range), Some(range_type.clone()))
            }
            None if ordered => return Err(format!("{name}: ordered queries need a range field")),
            None => (None, None),
        };
        let key = RecordKeyDefinition {
            kind: if range_type.is_some() {
                RecordKeyKind::HashRange
            } else {
                RecordKeyKind::Hash
            },
            range_type,
            allow_absent: false,
        };
        key.range_profile()
            .map_err(|error| format!("{name}: {error}"))?;
        Ok(Self {
            name,
            ordered,
            identity,
            coordinates: Coordinates { hash, range },
            key,
        })
    }

    fn priority(&self) -> u8 {
        match (self.ordered, self.key.kind) {
            (false, RecordKeyKind::Hash) => 0,
            (false, RecordKeyKind::HashRange) => 1,
            (true, _) => 2,
        }
    }

    fn primary_reason(&self) -> &'static str {
        match self.priority() {
            0 => "A record can be addressed directly by its stable ID.",
            1 => "A record can be addressed directly when both stable ID fields are known.",
            _ => {
                "The app guarantees that the ordered hash and range together identify each record."
            }
        }
    }
}

struct Layout<'a, 'q> {
    primary: &'q Query<'a>,
    alternates: BTreeMap<Coordinates<'a>, &'q Query<'a>>,
}

impl<'a, 'q> Layout<'a, 'q> {
    fn for_queries(primary: &'q Query<'a>, queries: &'q [Query<'a>]) -> Self {
        let mut alternates = BTreeMap::new();
        for query in queries {
            if query.coordinates != primary.coordinates {
                // Different queries with the same coordinates share one key.
                alternates.entry(query.coordinates).or_insert(query);
            }
        }
        Self {
            primary,
            alternates,
        }
    }

    fn summary(&self, queries: &[Query<'a>]) -> Value {
        let query_keys: BTreeMap<_, _> = queries
            .iter()
            .map(|query| {
                let key = self
                    .alternates
                    .get(&query.coordinates)
                    .map_or("primary", |entry| entry.name);
                (query.name, key)
            })
            .collect();
        json!({
            "primary_query": self.primary.name,
            "primary_key": self.primary.key,
            "primary_fields": self.primary.coordinates.mapping(),
            "reason": self.primary.primary_reason(),
            "alternate_key_count": self.alternates.len(),
            "query_keys": query_keys,
        })
    }

    fn proposal(&self, draft: &Draft) -> Result<Value, String> {
        draft.proposal(
            RecordKeys {
                primary: self.primary.key.clone(),
                alternates: self
                    .alternates
                    .values()
                    .map(|query| (query.name.to_owned(), query.key.clone()))
                    .collect(),
            },
            json!({
                "primary": self.primary.coordinates.mapping(),
                "alternates": self.alternates.values().map(|query| {
                    (query.name, query.coordinates.mapping())
                }).collect::<BTreeMap<_, _>>(),
            }),
        )
    }
}

pub(crate) fn schema(draft: &Value, input: &Value, primary: Option<&str>) -> Result<Value, String> {
    let requested_layout = draft
        .get("schema_type")
        .map(|kind| match kind.as_str() {
            Some("Hash") => Ok(RecordKeyKind::Hash),
            Some("HashRange") => Ok(RecordKeyKind::HashRange),
            _ => Err("choose Hash or HashRange for schema_type, or omit it to compare both"),
        })
        .transpose()?;
    let draft = Draft::parse(draft)?;
    if draft.definition.record_keys.is_some()
        || draft.definition.key.is_some()
        || !draft.definition.alternate_keys.is_empty()
    {
        return Err("recommend needs a new draft without declared keys; review existing key changes as a migration".into());
    }
    for (name, field_type) in &draft.definition.field_types {
        if contains_any(field_type) {
            return Err(format!("choose a concrete type for {name}"));
        }
    }

    let queries = input
        .as_array()
        .ok_or("queries must be a JSON array")?
        .iter()
        .enumerate()
        .map(|(index, value)| Query::parse(value, &draft, index))
        .collect::<Result<Vec<_>, _>>()?;
    if queries.is_empty() {
        return Err("describe at least one query for the record".into());
    }
    let mut names = BTreeSet::new();
    for query in &queries {
        if !names.insert(query.name) {
            return Err(format!("query {} is repeated", query.name));
        }
    }
    let mut candidates: Vec<_> = queries
        .iter()
        .filter(|query| {
            query.identity && requested_layout.is_none_or(|kind| query.key.kind == kind)
        })
        .collect();
    candidates.sort_by_key(|query| {
        (
            primary.is_some_and(|name| name != query.name),
            query.priority(),
        )
    });
    let chosen = candidates.first().copied().ok_or(if requested_layout.is_some() {
        "the requested schema type needs an identity query with the same key shape"
    } else {
        "mark a query identity: true when its coordinates are stable, always available, and unique for every record"
    })?;
    if primary.is_some_and(|name| name != chosen.name) {
        return Err(
            "--primary must name an identity query compatible with the requested schema type"
                .into(),
        );
    }
    let selected = Layout::for_queries(chosen, &queries);
    let proposal = selected.proposal(&draft)?;
    let mut primary_addresses = BTreeSet::from([chosen.coordinates]);
    let layouts = candidates
        .iter()
        .skip(1)
        .filter(|candidate| primary_addresses.insert(candidate.coordinates))
        .map(|candidate| Layout::for_queries(candidate, &queries).summary(&queries))
        .collect::<Vec<_>>();

    Ok(json!({
        "recommended": selected.summary(&queries),
        "alternatives": layouts,
        "declaration": proposal["declaration"],
        "app_supplies_on_write": proposal["app_supplies_on_write"],
        "notes": [
            "Identity choices rely on the app's guarantee of stable, unique coordinates. Lookups on other keys can return multiple records.",
            "All key coordinates must be supplied on each write. Use the returned primary key when changing a record.",
            "An ordered query uses a typed range under one hash. Equal alternate range values can tie.",
            "Without a requested primary, the advisor prefers a single-field identity, then a known pair for exact lookup. Query order breaks ties."
        ],
    }))
}

fn contains_any(field_type: &FieldValueType) -> bool {
    match field_type {
        FieldValueType::Any => true,
        FieldValueType::Array(item) | FieldValueType::Map(item) => contains_any(item),
        FieldValueType::Object(fields) => fields.values().any(contains_any),
        FieldValueType::OneOf(variants) => variants.iter().any(contains_any),
        _ => false,
    }
}
