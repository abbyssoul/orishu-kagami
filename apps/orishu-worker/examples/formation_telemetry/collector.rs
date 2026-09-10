//! Finite streaming receipt summaries: no retained span history or vendor SDK.
use super::Error;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use prost::Message;
use serde::Serialize;
use std::{io::Write, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinSet,
};

#[derive(Default, Serialize)]
pub(super) struct Receipt {
    batches: u64,
    bytes: u64,
    client: u64,
    peer: u64,
    admission: u64,
}

fn decode(body: &[u8]) -> Result<Receipt, Error> {
    let request = ExportTraceServiceRequest::decode(body)?;
    if request.resource_spans.len() != 1 || request.resource_spans[0].scope_spans.len() != 1 {
        return Err("unexpected resource/scope catalogue".into());
    }
    let spans = &request.resource_spans[0].scope_spans[0].spans;
    if spans.is_empty() || spans.len() > 128 {
        return Err("span batch outside profile".into());
    }
    let mut receipt = Receipt {
        batches: 1,
        bytes: body.len() as u64,
        ..Default::default()
    };
    for span in spans {
        if span.trace_id.len() != 16
            || span.trace_id.iter().all(|n| *n == 0)
            || span.span_id.len() != 8
            || span.span_id.iter().all(|n| *n == 0)
            || !(span.parent_span_id.is_empty() || span.parent_span_id.len() == 8)
            || span.end_time_unix_nano < span.start_time_unix_nano
            || span.start_time_unix_nano == 0
            || span.attributes.len() != 1
            || span.attributes[0].key != "orishu.outcome"
        {
            return Err("invalid received span contract".into());
        }
        use opentelemetry_proto::tonic::common::v1::any_value::Value;
        match span.attributes[0]
            .value
            .as_ref()
            .and_then(|value| value.value.as_ref())
        {
            Some(Value::StringValue(value))
                if matches!(
                    value.as_str(),
                    "completed" | "rejected" | "failed" | "cancelled"
                ) => {}
            _ => return Err("invalid span outcome".into()),
        }
        match span.name.as_str() {
            "orishu.client.request" => receipt.client += 1,
            "orishu.peer.exchange" => receipt.peer += 1,
            "orishu.admission" => receipt.admission += 1,
            _ => return Err("unknown span catalogue entry".into()),
        }
    }
    Ok(receipt)
}

async fn exchange(mut stream: TcpStream, workers: usize) -> Result<(usize, Receipt), Error> {
    let mut head = Vec::with_capacity(4096);
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() == 4096 {
            return Err("collector header cap".into());
        }
        head.push(stream.read_u8().await?);
    }
    let text = std::str::from_utf8(&head)?;
    let mut lines = text.split("\r\n");
    let role: usize = lines
        .next()
        .and_then(|line| line.strip_prefix("POST /v1/traces/"))
        .and_then(|line| line.strip_suffix(" HTTP/1.1"))
        .ok_or("unexpected collector request target")?
        .parse()?;
    if role >= workers {
        return Err("collector role outside formation".into());
    }
    let mut length = None;
    for line in lines.filter(|line| !line.is_empty()) {
        let (key, value) = line.split_once(':').ok_or("malformed HTTP header")?;
        if key.eq_ignore_ascii_case("transfer-encoding") {
            return Err("transfer encoding not in profile".into());
        }
        if key.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err("duplicate content length".into());
            }
            length = Some(value.trim().parse::<usize>()?);
        }
    }
    let length = length.ok_or("missing content length")?;
    if length > 1_048_576 {
        return Err("collector body cap".into());
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await?;
    let receipt = decode(&body)?;
    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await?;
    Ok((role, receipt))
}

pub(super) async fn run(workers: usize) -> Result<(), Error> {
    if !matches!(workers, 3 | 10 | 30) {
        return Err("unsupported collector worker count".into());
    }
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    println!("{}", listener.local_addr()?.port());
    std::io::stdout().flush()?;
    let mut stop = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut snapshot =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1())?;
    let mut tasks = JoinSet::new();
    let mut receipts: Vec<Receipt> = (0..workers).map(|_| Receipt::default()).collect();
    loop {
        tokio::select! {
            _ = stop.recv() => break,
            _ = snapshot.recv() => {
                println!("{}", serde_json::json!({"schema_version": 2, "workers": receipts}));
                std::io::stdout().flush()?;
            },
            connection = listener.accept(), if tasks.len() < 64 => {
                let (stream, _) = connection?;
                tasks.spawn(async move { tokio::time::timeout(Duration::from_secs(2), exchange(stream, workers)).await? });
            },
            Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                let (role, part) = result??;
                let total = &mut receipts[role];
                total.batches += part.batches;
                total.bytes += part.bytes;
                total.client += part.client;
                total.peer += part.peer;
                total.admission += part.admission;
            }
        }
    }
    // Runner stops all workers before the collector. Outstanding receipts are
    // a failed shutdown boundary, not silently omitted successful delivery.
    while let Some(result) = tasks.join_next().await {
        let (role, part) = result??;
        let total = &mut receipts[role];
        total.batches += part.batches;
        total.bytes += part.bytes;
        total.client += part.client;
        total.peer += part.peer;
        total.admission += part.admission;
    }
    println!(
        "{}",
        serde_json::json!({"schema_version": 2, "workers": receipts})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded_catalogue() -> Vec<u8> {
        use orishu_worker::trace_export::{BatchLimits, Operation, Outcome, SpanRecord};
        let records: Vec<_> = [
            Operation::ClientRequest,
            Operation::PeerExchange,
            Operation::Admission,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, operation)| {
            SpanRecord::new(
                [1; 16],
                [index as u8 + 1; 8],
                None,
                operation,
                1,
                2,
                Outcome::Completed,
            )
            .unwrap()
        })
        .collect();
        BatchLimits::new(128, 1_048_576)
            .unwrap()
            .encode(&records)
            .unwrap()
            .body()
            .to_vec()
    }

    #[test]
    fn production_encoder_catalogue_is_decoded_not_assumed() {
        let result = decode(&encoded_catalogue()).unwrap();
        assert_eq!((result.client, result.peer, result.admission), (1, 1, 1));
    }

    #[test]
    fn unknown_and_invalid_spans_are_not_counted_as_delivery() {
        for invalid_name in [false, true] {
            let mut request =
                ExportTraceServiceRequest::decode(encoded_catalogue().as_slice()).unwrap();
            let span = &mut request.resource_spans[0].scope_spans[0].spans[0];
            if invalid_name {
                span.name = "unknown".into();
            } else {
                span.trace_id.clear();
            }
            assert!(decode(&request.encode_to_vec()).is_err());
        }
    }

    #[tokio::test]
    async fn real_http_path_accepts_protobuf_and_rejects_duplicate_length() {
        tokio::time::timeout(Duration::from_secs(5), async {
            for duplicate in [false, true] {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let server =
                    tokio::spawn(
                        async move { exchange(listener.accept().await.unwrap().0, 3).await },
                    );
                let mut client = TcpStream::connect(address).await.unwrap();
                let body = encoded_catalogue();
                let extra = if duplicate {
                    "Content-Length: 1\r\n"
                } else {
                    ""
                };
                client
                    .write_all(
                        format!(
                            "POST /v1/traces/2 HTTP/1.1\r\nContent-Length: {}\r\n{extra}\r\n",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                client.write_all(&body).await.unwrap();
                if duplicate {
                    assert!(server.await.unwrap().is_err());
                } else {
                    let mut response = Vec::new();
                    client.read_to_end(&mut response).await.unwrap();
                    assert!(response.starts_with(b"HTTP/1.1 200 OK"));
                    assert_eq!(server.await.unwrap().unwrap().0, 2);
                }
            }
        })
        .await
        .unwrap();
    }
    #[test]
    fn malformed_and_empty_protobuf_are_not_receipts() {
        assert!(decode(&[0xff]).is_err());
        assert!(decode(&[]).is_err());
    }
}
