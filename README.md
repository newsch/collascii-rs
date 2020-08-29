# collascii-rs

This is the beginnings of a rewrite of [`collascii`](https://github.com/olin/collascii) in [rust](https://www.rust-lang.org/).

At this point the most interesting piece is [`server.rs`](src/bin/server.rs), a multi-threaded (and non-segfaulting!) server that's compatible with the original collascii.

This repository began life as a branch on the collascii repo.
