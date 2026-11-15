use clap::ValueEnum;
use serde_json::{json, Map, Value};
use std::time::Duration;

const MAX_REQUEST_BYTES: usize = 128 * 1024;
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Default, ValueEnum)]
pub(crate) enum Environment {
    Dev,
    #[default]
    Prod,
}

impl Environment {
    fn advice_url(self) -> &'static str {
        // Official Schema Service gateways from LastDB's environments.json.
        match self {
            Self::Dev => "https://y0q3m6vk75.execute-api.us-west-2.amazonaws.com/v1/advice/schema",
            Self::Prod => "https://axo709qs11.execute-api.us-east-1.amazonaws.com/v1/advice/schema",
        }
    }
}

pub(crate) struct CloudAdvice {
    agent: ureq::Agent,
    url: &'static str,
    unavailable: Option<&'static str>,
}

impl CloudAdvice {
    pub(crate) fn new(environment: Environment) -> Self {
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            .proxy(None)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(5)))
            .http_status_as_error(false)
            .build()
            .into();
        Self {
            agent,
            url: environment.advice_url(),
            unavailable: None,
        }
    }

    pub(crate) fn findings(
        &mut self,
        app_id: Option<&str>,
        schema: &Value,
    ) -> Result<Vec<Value>, &'static str> {
        if let Some(reason) = self.unavailable {
            return Err(reason);
        }
        let schema = schema
            .as_object()
            .ok_or("Schema Service needs a schema object.")?;
        // Send the declaration fields used by advice. Other metadata and any
        // example record values stay in the local document.
        let draft: Map<_, _> = [
            "name",
            "fields",
            "field_types",
            "field_descriptions",
            "field_versions",
            "field_classifications",
            "field_data_classifications",
        ]
        .into_iter()
        .filter_map(|key| schema.get(key).map(|value| (key.to_owned(), value.clone())))
        .collect();
        let body = serde_json::to_vec(&json!({"app_id": app_id, "schema": draft}))
            .map_err(|_| "The draft could not be serialized for Schema Service.")?;
        if body.len() > MAX_REQUEST_BYTES {
            return Err("The draft exceeds Schema Service's 128 KiB request limit.");
        }

        let mut response = match self
            .agent
            .post(self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send(body.as_slice())
        {
            Ok(response) => response,
            Err(_) => return self.stop("Schema Service could not be reached within five seconds."),
        };
        match response.status().as_u16() {
            200 => {}
            400 | 413 | 422 => return Err("Schema Service could not review this draft."),
            429 => return self.stop("Schema Service rate limited this review. Try again later."),
            _ => return self.stop("Schema Service returned an unexpected HTTP status."),
        }
        let Ok(body) = response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_vec()
        else {
            return self.stop("Schema Service returned an unreadable or oversized response.");
        };
        let Ok(mut reply) = serde_json::from_slice::<Value>(&body) else {
            return self.stop("Schema Service returned invalid JSON.");
        };
        let Some(Value::Array(findings)) = reply.get_mut("findings").map(Value::take) else {
            return self.stop("Schema Service returned no findings list.");
        };
        if findings
            .iter()
            .any(|finding| !finding["kind"].is_string() || !finding["message"].is_string())
        {
            return self.stop("Schema Service returned an invalid finding.");
        }
        Ok(findings)
    }

    fn stop(&mut self, reason: &'static str) -> Result<Vec<Value>, &'static str> {
        self.unavailable = Some(reason);
        Err(reason)
    }
}
