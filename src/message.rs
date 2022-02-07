use actix::Message as _;
use serde::{Serialize, Deserialize, Serializer, Deserializer, ser::SerializeStruct};

use json::Value;

/// Deserializer for `Option<Value>` that produces `Some(Value::Null)`.
///
/// The usual one produces None in that case. But we need to know the difference between
/// `{x: null}` and `{}`.
fn id_not_null<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Value, D::Error> {
    let v: Value = Deserialize::deserialize(deserializer)?;
    if v.is_null() {
        return Err(serde::de::Error::custom("Id can't be null"));
    }
    return Ok(v);
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub method: String,
    pub params: Value,

    #[serde(deserialize_with = "id_not_null")]
    pub id: Value,
}


#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub result: Result<Value, Value>,
    pub id: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub method: String,
    pub params: Value,
}


impl Request {
    pub fn reply(&self, reply: Value) -> Response {
        Response {
            result: Ok(reply),
            id: self.id.clone(),
        }
    }
    pub fn error(&self, error: Value) -> Response {
        Response {
            result: Err(error),
            id: self.id.clone(),
        }
    }
}


impl Serialize for Response {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.result {
            Ok(ref value) => {
                let mut sub = serializer.serialize_struct("Response", 2)?;
                sub.serialize_field("id", &self.id)?;
                sub.serialize_field("result", value)?;
                sub.end()
            }
            Err(ref err) => {
                let mut sub = serializer.serialize_struct("Response", 2)?;
                sub.serialize_field("id", &self.id)?;
                sub.serialize_field("result", &Value::Null)?;
                sub.serialize_field("error", err)?;
                sub.end()
            }
        }
    }
}


// Implementing deserialize is hard. We sidestep the difficulty by deserializing a similar
// structure that directly corresponds to whatever is on the wire and then convert it to our more
// convenient representation.
impl<'de> Deserialize<'de> for Response {
    #[allow(unreachable_code)] // For that unreachable below
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// A helper trick for deserialization.
        #[derive(Deserialize)]
        struct WireResponse {
            // It is actually used to eat and sanity check the deserialized text
            #[allow(dead_code)]
            #[serde(default)]
            result: Option<Value>,
            #[serde(default)]
            error: Option<Value>,
            #[serde(deserialize_with = "id_not_null")]
            id: Value,
        }

        let wr: WireResponse = Deserialize::deserialize(deserializer)?;
        let result = match (wr.result, wr.error) {
            (Some(res), None) => Ok(res),
            (Some(Value::Bool(false)), Some(err)) => {
                Err(err)
            }
            (None, Some(err)) => Err(err),
            _ => {
                let err = serde::de::Error::custom("Either 'error' or 'result' is expected, but not both");
                return Err(err);
            }
        };
        Ok(Response {
            result: result,
            id: wr.id,
        })
    }
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    /// An RPC request.
    Request(Request),
    /// A response to a Request.
    Response(Response),
    /// A notification.
    Notification(Notification),
    /// A batch of more requests or responses.
    ///
    /// The protocol allows bundling multiple requests, notifications or responses to a single
    /// message.
    ///
    /// This variant has no direct constructor and is expected to be constructed manually.
    Batch(Vec<Message>),
    /// An unmatched sub entry in a `Batch`.
    ///
    /// When there's a `Batch` and an element doesn't comform to the JSONRPC 2.0 format, that one
    /// is represented by this. This is never produced as a top-level value when parsing, the
    /// `Err(Broken::Unmatched)` is used instead. It is not possible to serialize.
    #[serde(skip_serializing)]
    UnmatchedSub(Value),
    #[serde(skip_serializing)]
    Dummy,
}


impl Into<Message> for Request {
    fn into(self) -> Message {
        Message::Request(self)
    }
}

impl Into<Message> for Response {
    fn into(self) -> Message {
        Message::Response(self)
    }
}

impl Into<Message> for Notification {
    fn into(self) -> Message {
        Message::Notification(self)
    }
}