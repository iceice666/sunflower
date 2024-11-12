include!(concat!(env!("OUT_DIR"), "/protocol.rs"));

use std::collections::HashMap;
use std::fmt::Display;

impl Response {
    pub fn ok(data: Option<String>) -> Self {
        Self {
            r#type: ResponseType::Ok.into(),
            payload: data.map(response::Payload::Data),
        }
    }

    pub fn err(error: String) -> Self {
        Self {
            r#type: ResponseType::Error.into(),
            payload: Some(response::Payload::Error(error)),
        }
    }
}

impl From<HashMap<String, &HashMap<String, String>>> for SearchResults {
    fn from(value: HashMap<String, &HashMap<String, String>>) -> Self {
        let results = value
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    ProviderSearchResult {
                        values: v
                            .clone()
                            .into_iter()
                            .map(|(k, v)| (k, v.to_string()))
                            .collect(),
                    },
                )
            })
            .collect();

        Self { results }
    }
}

impl Display for RepeatState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepeatState::None => write!(f, "None"),
            RepeatState::Queue => write!(f, "Queue"),
            RepeatState::Track => write!(f, "Track"),
        }
    }
}
