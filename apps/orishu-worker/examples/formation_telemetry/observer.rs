//! Diagnostic observer, not an alternative acceptance workload or worker API.
use super::Error;
use orishu::client::{
    ClientApi, ClusterAddress, ClusterApi, MembershipApi,
    http_client::{HttClientOptions, HttpClusterClient},
};
use orishu::model::cluster::MembersSelector;
use serde::Deserialize;
use std::{
    io::{BufRead, Read, Write},
    path::PathBuf,
    time::Duration,
};

const COMMAND_BYTES: u64 = 128;
const REPLY_BYTES: usize = 1_048_576;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    role: usize,
    operation: Operation,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Summary,
    Members,
}

fn query(bytes: &[u8], workers: usize) -> Result<Query, Error> {
    if bytes.len() as u64 >= COMMAND_BYTES || !bytes.ends_with(b"\n") {
        return Err("unterminated or oversized observer command".into());
    }
    let query: Query = serde_json::from_slice(bytes)?;
    if query.role >= workers {
        return Err("observer role outside fixture".into());
    }
    Ok(query)
}

struct Reply(Vec<u8>);

impl Write for Reply {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > REPLY_BYTES - self.0.len() {
            return Err(std::io::Error::other("observer reply cap"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) async fn run(root: PathBuf, workers: usize) -> Result<(), Error> {
    if !matches!(workers, 3 | 10 | 30) {
        return Err("unsupported observer worker count".into());
    }
    let clients = (0..workers)
        .map(|role| {
            HttpClusterClient::new(
                ClusterAddress::UnixSocket(root.join(role.to_string()).join("api.sock")),
                HttClientOptions {
                    timeout: Some(Duration::from_secs(2)),
                    ..Default::default()
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    output.write_all(b"READY\n")?;
    output.flush()?;
    let mut command = Vec::with_capacity(COMMAND_BYTES as usize);
    let mut reply = Reply(Vec::with_capacity(REPLY_BYTES));
    for _ in 0..4096 {
        command.clear();
        // Blocking stdin runs on main, not the two runtime worker threads.
        // The owning diagnostic enforces the process deadline and closes stdin.
        if input
            .by_ref()
            .take(COMMAND_BYTES)
            .read_until(b'\n', &mut command)?
            == 0
        {
            return Ok(());
        }
        let query = query(&command, workers)?;
        reply.0.clear();
        let cluster = clients[query.role].cluster();
        tokio::time::timeout(Duration::from_secs(2), async {
            match query.operation {
                Operation::Summary => serde_json::to_writer(&mut reply, &cluster.summary().await?)?,
                Operation::Members => serde_json::to_writer(
                    &mut reply,
                    &clients[query.role]
                        .membership()
                        .list(&MembersSelector::default())
                        .await?,
                )?,
            }
            Ok::<_, Error>(())
        })
        .await??;
        output.write_all(&reply.0)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Err("observer command count cap".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_bounded_read_only_and_fixture_scoped() {
        assert!(query(b"{\"role\":29,\"operation\":\"members\"}\n", 30).is_ok());
        for bytes in [
            &b"{\"role\":30,\"operation\":\"summary\"}\n"[..],
            b"{\"role\":0,\"operation\":\"unlock\"}\n",
            b"{\"role\":0,\"operation\":\"summary\",\"extra\":1}\n",
            b"{\"role\":0,\"operation\":\"summary\"}",
            &[b' '; 128],
        ] {
            assert!(query(bytes, 30).is_err());
        }
    }

    #[test]
    fn reply_cap_prevents_growth_beyond_declared_bound() {
        let mut reply = Reply(vec![0; REPLY_BYTES - 1]);
        assert_eq!(reply.write(b"x").unwrap(), 1);
        assert!(reply.write(b"x").is_err());
        assert_eq!(reply.0.len(), REPLY_BYTES);
    }
}
