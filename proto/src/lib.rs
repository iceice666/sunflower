include!(concat!(env!("OUT_DIR"), "/protocol.rs"));
pub use prost::DecodeError;
use prost::Message;

pub fn serilize_response(response: Response) -> Vec<u8> {
    let mut buf = Vec::with_capacity(response.encoded_len());
    response.encode(&mut buf).unwrap();
    buf
}

pub fn deserilize_response(buf: &[u8]) -> Result<Response, prost::DecodeError> {
    Response::decode(buf)
}

pub fn serilize_request(request: Request) -> Vec<u8> {
    let mut buf = Vec::with_capacity(request.encoded_len());
    request.encode(&mut buf).unwrap();
    buf
}

pub fn deserilize_request(buf: &[u8]) -> Result<Request, prost::DecodeError> {
    Request::decode(buf)
}
