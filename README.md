# simple-gossip
This project is based on tentacle https://github.com/nervosnetwork/tentacle


## Example

Start three nodes:

Node A

```
RUST_LOG=info cargo run 1 3000
```

Start node B and connect to A

```
RUST_LOG=info cargo run 2 3001 /ip4/127.0.0.1/tcp/3000
```

Start node C and connect to A

```
RUST_LOG=info cargo run 3 3002 /ip4/127.0.0.1/tcp/3000
```

From the above 3 commands, A direct connect to B, A direct connect to C. At the last, the 3 nodes will direct connect to each other via simple-gossip

//TODO: braodcast meesages
