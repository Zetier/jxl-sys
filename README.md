# jxl-sys

Rust bindings to [libjxl](https://github.com/libjxl/libjxl)

## Building
Before building, you will need to make sure that git submodules are initialized:
```bash
git submodule update --init --recursive
```

You will also need to make sure you have all prerequisites to build `libjxl`.
Refer to `libjxl/BUILDING.md` for detailed instructions.

Then you can build with `cargo`:
```bash
cargo build
```

## License
`jxl-sys` is primarily distributed under the terms of both the MIT license and
the Apache License (Version 2.0).

See LICENSE-APACHE, LICENSE-MIT for details.

Copyright (c) 2025 Zetier
