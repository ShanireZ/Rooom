FROM rust:1.100.0-bookworm@sha256:ade39a710ef85f3506cffcfec3e8a24ba090badef1dff2fb798f71d878833fea AS test
WORKDIR /rooom
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src-tauri/Cargo.toml src-tauri/build.rs src-tauri/
COPY crates/rooom-core/ crates/rooom-core/
COPY crates/rooom-platform/ crates/rooom-platform/
RUN cargo test --locked -p rooom-core --no-run
CMD ["cargo", "test", "--locked", "--offline", "-p", "rooom-core"]
