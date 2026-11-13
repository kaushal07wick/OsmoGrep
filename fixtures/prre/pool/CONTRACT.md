# Controlled pool scenario (synthetic, not a public-repository benchmark)

Every configured worker must acquire one pool slot without waiting while all
workers hold their slots. A worker batch returns true exactly when this contract
holds. The barrier represents a batch operation that cannot release its resources
until every participant has attempted acquisition.

The old smoke test checks one acquisition. It intentionally misses contention.
PR A changes pool capacity from 16 to 8; PR B changes worker concurrency from 8
to 16. Each change alone satisfies the contract. Their combination violates it.
The independent oracle is this contract, written before either change.
