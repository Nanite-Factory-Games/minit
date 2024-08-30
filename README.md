# Minit

A simple init for containers, inspired by tini and initrs.
It handles zombie reaping and signal forwarding in as simple a way as possible.
This shares most of its code with initrs, but has been updated with newer
dependencies and a different approach to how the init process is spawned.

## Why?
For most use cases, you probably don't need to use minit. This is meant to be used
in conjunction with another project called [whaledrive](https://github.com/Nanite-Factory-Games/whaledrive). The design of this project is such that whaledrive can use minit to
make a vm act similar to how a container would act.

Docker images have some things defined that are not present on the filesystem,
so when when you convert an image to a vm, you need to have a way to pass those
things to the vm. This is where minit comes in.

The environment variables from the docker image are put into a key value map
in /etc/environment.json. This file is then read by minit and the environment
variables are loaded into the system.

The entrypoint and command are also not present on the filesystem, so they need
to be passed via environment variables.


## Building
To build minit, you can either build it yourself or use the docker image
provided in the docker directory.

### Docker

```sh
docker build -t build-linux-x86-64 .
docker run -v ./:/mnt/src build-linux-x86-64
```

### Building Locally

Prerequisites:
- Rust
- Cargo
- glibc static libraries

```sh
RUSTFLAGS="-C target-feature=+crt-static" cargo build --target x86_64-unknown-linux-gnu --release
```


## License

Since initrs is licensed under the terms of the GNU GPLv3 (or later), this
project is licensed under the same terms.