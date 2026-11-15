use schema_types::{
    record_keys::{RecordKeyDefinition, RecordKeyKind, RecordKeys},
    DeclarativeSchemaDefinition, DeclarativeSchemaType, FieldValueType,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(crate) struct Draft {
    input: Map<String, Value>,
    pub(crate) definition: DeclarativeSchemaDefinition,
}

impl Draft {
    pub(crate) fn parse(draft: &Value) -> Result<Self, String> {
        let mut input = draft
            .as_object()
            .cloned()
            .ok_or("expected a schema object")?;
        let name = input
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if name.trim().is_empty() {
            return Err("give the schema a name".into());
        }
        input.entry("schema_type").or_insert(json!("Hash"));
        let definition: DeclarativeSchemaDefinition =
            serde_json::from_value(Value::Object(input.clone()))
                .map_err(|error| error.to_string())?;
        let fields = definition
            .fields
            .as_deref()
            .ok_or("declare the record fields")?;
        for field in fields {
            if !definition.field_types.contains_key(field) {
                return Err(format!("choose a concrete type for {field}"));
            }
        }
        Ok(Self { input, definition })
    }

    pub(crate) fn field_type(&self, field: &str) -> Option<&FieldValueType> {
        self.definition
            .fields
            .as_ref()
            .is_some_and(|fields| fields.iter().any(|declared| declared == field))
            .then(|| self.definition.field_types.get(field))
            .flatten()
    }

    pub(crate) fn proposal(&self, keys: RecordKeys, mapping: Value) -> Result<Value, String> {
        keys.validate()?;
        if self
            .definition
            .record_keys
            .as_ref()
            .is_some_and(|existing| existing != &keys)
        {
            return Err(
                "the draft already declares a different key layout; review migration compatibility"
                    .into(),
            );
        }

        let mut candidate = self.definition.clone();
        candidate.schema_type = match keys.primary.kind {
            RecordKeyKind::Hash => DeclarativeSchemaType::Hash,
            RecordKeyKind::HashRange => DeclarativeSchemaType::HashRange,
        };
        candidate.record_keys = Some(keys);
        candidate.validate_definition()?;

        let mut declaration = self.input.clone();
        declaration.insert("schema_type".into(), json!(candidate.schema_type));
        declaration.insert("record_keys".into(), json!(candidate.record_keys));
        Ok(json!({
            "declaration": declaration,
            "app_supplies_on_write": mapping,
        }))
    }
}

pub(crate) fn schema(draft: &Value, id: &str, ordered: &[String]) -> Result<Value, String> {
    if draft.get("schema_type").is_some_and(|kind| kind != "Hash") {
        return Err("this planner needs a Hash primary layout".into());
    }
    let draft = Draft::parse(draft)?;
    if draft.field_type(id) != Some(&FieldValueType::String) {
        return Err("choose a stable, unique String field for the primary hash".into());
    }
    let mut alternates = BTreeMap::new();
    let mut addresses = Map::new();
    for query in ordered.chunks_exact(3) {
        let (name, hash, range) = (&query[0], &query[1], &query[2]);
        if draft.field_type(hash) != Some(&FieldValueType::String) {
            return Err(format!("{name} needs a declared String hash field"));
        }
        let range_type = draft
            .field_type(range)
            .ok_or_else(|| format!("{name} needs a declared range field"))?;
        let key = RecordKeyDefinition {
            kind: RecordKeyKind::HashRange,
            range_type: Some(range_type.clone()),
            allow_absent: false,
        };
        key.range_profile()
            .map_err(|error| format!("{name}: {error}"))?;
        if alternates.insert(name.clone(), key).is_some() {
            return Err(format!("alternate key {name} is repeated"));
        }
        addresses.insert(
            name.clone(),
            json!({ "hash_from": hash, "range_from": range }),
        );
    }
    draft.proposal(
        RecordKeys {
            primary: RecordKeyDefinition {
                kind: RecordKeyKind::Hash,
                range_type: None,
                allow_absent: false,
            },
            alternates,
        },
        json!({
            "primary": { "hash_from": id },
            "alternates": addresses,
        }),
    )
}
