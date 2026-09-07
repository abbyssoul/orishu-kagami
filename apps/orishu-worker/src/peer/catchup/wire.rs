//! Profile-4 reliable admission-state exchanges. These are IO-shell resources,
//! not membership gossip or replayable core messages. No payload-bearing Debug.
use super::{
    Descriptor, Page,
    store::{self, Store},
};
use crate::{credentials::SecretToken, driver::Generation, peer::codec};
use orishu::model::cluster::{AdmissionToken, OperationId};
use orishu_membership::{FormationId, Membership, NodeId, PeerContext, SenderIdentity};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Correlated read/continuation request on an admitted bidirectional stream.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub schema_version: u8,
    #[serde(rename = "type")]
    kind: String,
    pub formation_id: FormationId,
    pub sender_id: NodeId,
    pub request_id: OperationId,
    pub action: Action,
}

/// Every action is independently authenticated against the current session.
#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum Action {
    Begin,
    Page { snapshot: u64, index: u16 },
    Confirm { snapshot: u64, root: [u8; 32] },
}

/// Public response envelope; credentials appear only in the explicit confirmed
/// credential response and must never enter ordinary logs/debug output.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reply {
    pub schema_version: u8,
    #[serde(rename = "type")]
    kind: String,
    pub formation_id: FormationId,
    pub sender_id: NodeId,
    pub request_id: OperationId,
    pub outcome: Outcome,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "camelCase", deny_unknown_fields)]
pub enum Outcome {
    Baseline {
        descriptor: Descriptor,
    },
    Page {
        page: Page,
    },
    Credential {
        snapshot: u64,
        root: [u8; 32],
        token: AdmissionToken,
    },
    Rejected {
        reason: Rejection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Rejection {
    NotReady,
    Unavailable,
    Overloaded,
    Invalid,
    Unauthorized,
}

impl Request {
    pub fn new(
        formation_id: FormationId,
        sender_id: NodeId,
        request_id: OperationId,
        action: Action,
    ) -> Self {
        Self {
            schema_version: 1,
            kind: "AdmissionStateReq".into(),
            formation_id,
            sender_id,
            request_id,
            action,
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>, codec::CodecError> {
        codec::encode(self)
    }
}

impl Reply {
    /// Validate a reply's resource identity before consuming a page or token.
    /// The transport caller additionally pins the responding member certificate.
    pub fn decode(
        bytes: &[u8],
        formation: &FormationId,
        source: &NodeId,
        request: &OperationId,
    ) -> Result<Self, codec::CodecError> {
        if bytes.len() > 131_072 {
            return Err(codec::CodecError::TooLarge);
        }
        let reply: Self = codec::decode(bytes)?;
        if reply.schema_version != 1
            || reply.kind != "AdmissionStateReply"
            || &reply.formation_id != formation
            || &reply.sender_id != source
            || &reply.request_id != request
        {
            return Err(codec::CodecError::Schema);
        }
        Ok(reply)
    }
}

/// Development-only corruption after normal source authorization and encoding.
#[cfg(feature = "formation-fault-test")]
pub(crate) fn corrupt_page(
    bytes: Vec<u8>,
    index: u16,
) -> Result<(Vec<u8>, bool), codec::CodecError> {
    let mut reply: Reply = codec::decode(&bytes)?;
    let Outcome::Page { page } = &mut reply.outcome else {
        return Ok((bytes, false));
    };
    if page.index != index {
        return Ok((bytes, false));
    }
    // Preserve the authenticated envelope and valid CBOR. The real baseline
    // receiver must reject the unsupported page schema before adoption.
    page.schema_version = 0;
    Ok((codec::encode(&reply)?, true))
}

/// Identify this namespace without allocating the request's typed payload.
pub(crate) fn is_request(bytes: &[u8]) -> Result<bool, codec::CodecError> {
    for (field, value) in codec::record_fields(bytes)? {
        if field == "type" {
            return Ok(codec::decode::<String>(value)? == "AdmissionStateReq");
        }
    }
    Ok(false)
}

/// Current source authority assembled only inside the serialized owner.
pub(crate) struct Source<'a> {
    pub model: &'a Membership,
    pub generation: Generation,
    pub peer: &'a PeerContext,
    pub ready: bool,
    pub token: Option<&'a SecretToken>,
}

/// Handle a validated registered stream without putting any credential into the
/// core. Raw malformed/binding failures return no response containing secrets.
pub(crate) fn serve(
    store: &mut Store,
    source: Source<'_>,
    bytes: &[u8],
    now: Instant,
) -> Result<Vec<u8>, codec::CodecError> {
    if bytes.len() > 4096 {
        return Err(codec::CodecError::TooLarge);
    }
    let request: Request = codec::decode(bytes)?;
    if request.schema_version != 1
        || request.kind != "AdmissionStateReq"
        || request.formation_id != *source.model.formation()
        || request.formation_id != source.peer.formation
        || !source.peer.authenticated
        || source.peer.sender != SenderIdentity::Admitted(request.sender_id.clone())
    {
        return Err(codec::CodecError::Schema);
    }
    let outcome = if !source.ready || source.token.is_none() {
        Outcome::Rejected {
            reason: Rejection::NotReady,
        }
    } else {
        let result = match request.action {
            Action::Begin => store
                .begin(
                    source.model,
                    source.generation,
                    source.peer,
                    request.request_id.clone(),
                    now,
                )
                .map(|descriptor| Outcome::Baseline { descriptor }),
            Action::Page { snapshot, index } => store
                .page(
                    source.model,
                    source.generation,
                    source.peer,
                    snapshot,
                    index,
                    now,
                )
                .and_then(|page| {
                    codec::decode(page)
                        .map(|page| Outcome::Page { page })
                        .map_err(|error| store::Error::Baseline(error.into()))
                }),
            Action::Confirm { snapshot, root } => store
                .confirm(
                    source.model,
                    source.generation,
                    source.peer,
                    snapshot,
                    root,
                    now,
                )
                .map(|()| Outcome::Credential {
                    snapshot,
                    root,
                    token: source
                        .token
                        .expect("checked current token")
                        .expose()
                        .to_owned()
                        .try_into()
                        .expect("valid token"),
                }),
        };
        result.unwrap_or_else(|error| Outcome::Rejected {
            reason: match error {
                store::Error::Unauthorized => Rejection::Unauthorized,
                store::Error::Unavailable => Rejection::Unavailable,
                store::Error::Overloaded => Rejection::Overloaded,
                store::Error::Invalid | store::Error::Baseline(_) => Rejection::Invalid,
            },
        })
    };
    codec::encode(&Reply {
        schema_version: 1,
        kind: "AdmissionStateReply".into(),
        formation_id: source.model.formation().clone(),
        sender_id: source.model.local_id().clone(),
        request_id: request.request_id,
        outcome,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use orishu_membership::testing;

    #[test]
    fn owner_binding_readiness_and_reply_correlation_fail_closed() {
        let model = testing::model_with_members(1);
        let peer = testing::peer_context(&model, &"node-0000".parse().unwrap(), 1);
        let token = SecretToken::generate().unwrap();
        let source = |ready| Source {
            model: &model,
            generation: Generation(0),
            peer: &peer,
            ready,
            token: Some(&token),
        };
        let request_id: OperationId = "request".parse().unwrap();
        let request = Request::new(
            model.formation().clone(),
            "node-0000".parse().unwrap(),
            request_id.clone(),
            Action::Begin,
        );
        let mut store = Store::default();
        let bytes = serve(
            &mut store,
            source(false),
            &request.encode().unwrap(),
            Instant::now(),
        )
        .unwrap();
        assert!(
            !bytes
                .windows(token.expose().len())
                .any(|window| window == token.expose().as_bytes())
        );
        assert!(matches!(
            Reply::decode(&bytes, model.formation(), model.local_id(), &request_id)
                .unwrap()
                .outcome,
            Outcome::Rejected {
                reason: Rejection::NotReady
            }
        ));
        assert!(
            Reply::decode(
                &bytes,
                model.formation(),
                model.local_id(),
                &"other".parse().unwrap()
            )
            .is_err()
        );
        let mut wrong = request;
        wrong.sender_id = model.local_id().clone();
        assert!(
            serve(
                &mut store,
                source(true),
                &wrong.encode().unwrap(),
                Instant::now()
            )
            .is_err()
        );
        wrong.sender_id = "node-0000".parse().unwrap();
        wrong.formation_id = "other".parse().unwrap();
        assert!(
            serve(
                &mut store,
                source(true),
                &wrong.encode().unwrap(),
                Instant::now()
            )
            .is_err()
        );
        assert!(serve(&mut store, source(true), &vec![0; 4097], Instant::now()).is_err());
    }
}
