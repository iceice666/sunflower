use std::collections::HashMap;

pub use prost::DecodeError;
use prost::Message;
use response::Payload;

include!(concat!(env!("OUT_DIR"), "/protocol.rs"));

impl Response {
    pub fn ok(data: Option<String>) -> Self {
        Self {
            r#type: ResponseType::Ok.into(),
            payload: data.map(|v| Payload::Data(v)),
        }
    }

    pub fn err(error: String) -> Self {
        Self {
            r#type: ResponseType::Error.into(),
            payload: Some(Payload::Error(error)),
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
                        values: v.clone().into_iter().map(|(k, v)| (k, v.to_string())).collect(),
                    },
                )
            })
            .collect();

        Self { results }
    }
}

pub fn serialize_response(response: Response) -> Vec<u8> {
    let mut buf = Vec::with_capacity(response.encoded_len());
    response.encode(&mut buf).unwrap();
    buf
}

pub fn deserialize_response(buf: &[u8]) -> Result<Response, DecodeError> {
    Response::decode(buf)
}

pub fn serialize_request(request: Request) -> Vec<u8> {
    let mut buf = Vec::with_capacity(request.encoded_len());
    request.encode(&mut buf).unwrap();
    buf
}

pub fn deserialize_request(buf: &[u8]) -> Result<Request, DecodeError> {
    Request::decode(buf)
}
