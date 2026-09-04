# 0015 - Use QUIC-native transfer for committed artifact chunks

Status: **accepted**

## Context

A BitTorrent-family transport was considered for large-artifact fan-out. Once
public discovery, peer exchange, emergent placement, and BitTorrent integrity
were removed to preserve Orishu's authenticated cluster semantics, the proposal
retained substantial parser and dependency cost for little unique value.

## Decision

Committed checkpoint and result chunks use bounded `FetchChunk` requests over
the existing authenticated, formation-scoped QUIC peer streams. `ChunkRef` and
SHA-256 content hashes are the single end-to-end identity and integrity
authority. There is no torrent, infohash, SHA-1 integrity layer, public swarm,
or separate discovery protocol.

Placement remains cluster policy: the consistent-hash ring, virtual nodes, and
replication target decide intended holders. Discovery uses the node-local
catalog derived from artifact-record gossip. Retrieval may fan out across live
holders, resume by verified ranges, and make a successfully fetched chunk
eligible for bounded opportunistic reseeding.

Live catch-up state remains distinct: partition transfer during late join or
ownership movement uses the correctness-bearing checkpoint/catch-up protocol,
whereas `FetchChunk` retrieves already committed immutable artifacts.

## Consequences

- No additional hostile-input parser or transport dependency is introduced.
- Holder claims remain advisory until bytes verify against the artifact record.
- Read repair uses verified bytes, not ring position as truth.
- A swarm transport may be reconsidered only if reproducible large-artifact
  benchmarks show a material benefit for Orishu's bounded, known membership.

## Related documents

- `docs/storage-model.md`
- `docs/storage-spec.md`
- `docs/protocol-p2p.md`
