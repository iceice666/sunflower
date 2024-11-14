mod proto;

pub use prost::DecodeError;
use prost::Message;

pub use proto::{
    request::Payload as RequestPayload, response::Payload as ResponsePayload, ProviderConfig,
    ProviderList, RepeatState, Request as PlayerRequest, RequestType, Response as PlayerResponse,
    ResponseType, SearchResults, TrackConfig, TrackData, TrackSearch,
};

pub fn serialize_response(response: PlayerResponse) -> Vec<u8> {
    let mut buf = Vec::with_capacity(response.encoded_len());
    response.encode(&mut buf).unwrap();
    buf
}

pub fn deserialize_response(buf: &[u8]) -> Result<PlayerResponse, DecodeError> {
    PlayerResponse::decode(buf)
}

pub fn serialize_request(request: PlayerRequest) -> Vec<u8> {
    let mut buf = Vec::with_capacity(request.encoded_len());
    request.encode(&mut buf).unwrap();
    buf
}

pub fn deserialize_request(buf: &[u8]) -> Result<PlayerRequest, DecodeError> {
    PlayerRequest::decode(buf)
}
